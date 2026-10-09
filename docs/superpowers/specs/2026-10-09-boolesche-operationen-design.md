# PlanoraCAD – Spec Teilprojekt 3: Boolesche Operationen

Datum: 2026-10-09 · Status: Entwurf zur Freigabe · Baut auf: `2026-10-09-fundament-und-kernel-design.md` (Teilprojekt 1+2, umgesetzt)

## 1. Ziel und Einordnung

Vereinigen, Abziehen und Schneiden zweier Körper im eigenen Rust-Kernel, robust genug für Alltags-Konstruktion (Bohrungen, Taschen, Nuten, Aussparungen, Zusammenbau von Teilen), mit Anbindung in der Oberfläche.

**Umfang (vom Projekteigner gewählt: „Stufe A“):** Es werden nur Flächenpaare unterstützt, deren Schnitt aus **Geraden, Kreisen und Ellipsen** besteht. Alle anderen Paare werden mit einer benannten Fehlermeldung abgelehnt; es wird niemals ein falsches Ergebnis geliefert. Allgemeine Flächenschnitte (Raumkurven) gehören zu einem späteren Teilprojekt mit Spline-Kurven (8+).

**Erfolgskriterium:** Das Ergebnis besteht den Validator, hat positives und rechnerisch richtiges Volumen, ein geschlossenes und durchgängig orientiertes Netz, und ist im Programm bedienbar.

### Einordnung in die Teilprojekte

| # | Teilprojekt | Stand |
|---|---|---|
| 1+2 | Fundament, Kernel-Basis | umgesetzt |
| **3** | **Boolesche Operationen** | **diese Spec** |
| 4 | Skizzen + Constraint-Solver | danach |
| 5–8+ | Feature-Baum, Fillets, Export, NURBS, … | später |

## 2. Getroffene Grundentscheidungen

- **Ansatz „schlank“:** Kanten behalten nur ihre 3D-Kurve. Die Beschneidung gekrümmter Flächen ergibt sich aus den Randschleifen; deren Parameterform (u,v) wird bei Bedarf über die exakte Umkehrabbildung der Fläche berechnet. Parameterkurven pro Kante und Fläche (wie OpenCascade) werden **nicht** gespeichert; sie lohnen sich erst mit Splines.
- **Ergebnis kann mehrere Schalen haben** (Körper zerfällt in getrennte Teile). `Solid` unterstützt Schalen bereits.
- **Neue Abhängigkeit:** `spade` 2.15 (eingeschränkte Delaunay-Triangulierung für beschnittene gekrümmte Flächen im Netz). Lizenz **MIT OR Apache-2.0**, GPL-3-kompatibel, am 2026-10-09 geprüft; Laufzeit-Abhängigkeiten `hashbrown`, `num-traits`, `robust`, `smallvec` (alle permissiv). `cargo deny` muss grün bleiben.
- Alle anderen Grundentscheidungen (reines Rust, GPL-3.0-only, `Tolerance` zentral, Lizenzen immer prüfen, kein Autodesk-Material) gelten unverändert.

## 3. Architektur

Neues Modul `planora-kernel/src/boolean/` (der Kernel kennt weiterhin keine Oberfläche, keine Grafik, keine Tessellierung):

| Teil | Aufgabe |
|---|---|
| `intersect` | Fläche ↔ Fläche → Schnittkurven; Kurve ↔ Fläche → Schnittpunkte (Nullstellensuche) |
| `split` | Flächen und Kanten entlang der Schnittkurven auftrennen (ebener Graph im Parameterraum) |
| `classify` | Flächenstücke als innen / außen / bündig (gleich- oder gegenläufig) zum anderen Körper einordnen |
| `select` | Je nach Operation die richtigen Stücke behalten, ggf. umdrehen |
| `sew` | Stücke zu einem neuen `Solid` zusammennähen (Ecken und Kanten verschweißen) |

**Öffentliche Schnittstelle** (Erweiterung des `Kernel`-Traits):

```rust
pub enum BoolOp { Union, Difference, Intersection }
fn boolean(&self, a: &Solid, b: &Solid, op: BoolOp) -> Result<Solid, BooleanError>;
```

**Fehler (`BooleanError`)**, jeweils mit verständlicher deutscher Meldung: `UnsupportedIntersection { a, b }` (Flächentypen), `AmbiguousContact`, `AmbiguousClassification`, `NotClosed`, `ResultInvalid`, `EmptyResult`, `InvalidInput` (Eingangskörper besteht den Validator nicht).

**Erweiterungen der Kernel-Grundlagen:**
- `Surface::implicit(p) -> f64`: Vorzeichen = innen/außen, 0 = auf der Fläche. Ebene `n·(p−o)`, Zylinder Achsenabstand − r, Kugel `|p−c| − r`, Kegel und Torus analog.
- `Surface::project(p) -> (u, v)` (Umkehrabbildung, geschlossen für alle fünf Typen) und `Curve::project(p) -> t`.
- `Surface::domain_info()`: Periodizität in u, Poldecken (Kugel: v = ±π/2; Kegel: Spitze).
- Punkt-in-Fläche-Test im Parameterraum (Windungszahl der Randschleifen).

**Änderungen in anderen Crates:**
- `planora-mesh`: Netz für beschnittene gekrümmte Flächen (siehe §7).
- `planora-app`: Dialog Vereinigen/Abziehen/Schneiden und „Körper verschieben“ (siehe §9).

## 4. Schnitte berechnen (Stufe A)

Unterstützte Paare (alles andere → `UnsupportedIntersection`):

| Paar | Ergebnis |
|---|---|
| Ebene ∩ Ebene | Gerade (parallel/identisch: bündiger Fall) |
| Ebene ∩ Zylinder | Kreis (Achse ⟂ Ebene), 0/1/2 Geraden (Achse ∥ Ebene), Ellipse (schräg) |
| Ebene ∩ Kegel | Kreis (⟂ Achse), 2 Geraden (Ebene enthält die Achse) |
| Ebene ∩ Kugel | Kreis |
| Ebene ∩ Torus | Kreise (⟂ Achse oder durch die Achse) |
| Kugel ∩ Kugel | Kreis |
| Zylinder, Kegel, Kugel, Torus **mit gleicher Achse** | Kreise um diese Achse |
| Zylinder ∩ Zylinder, parallele Achsen | 0/1/2 Geraden |

**Methode:**
- Die Schnittkurve eines Paars kommt aus der geschlossenen Formel der Tabelle.
- **Wo die Kurve ein Flächenstück verlässt:** Eine Randkante `e` von Fläche A liegt auf der Oberfläche von A; ihr Schnittpunkt mit der Schnittkurve ist dort, wo `e` die Oberfläche von B trifft. Das ist die Nullstellensuche `F_B(e(t)) = 0` entlang der Kante (abtasten, Vorzeichenwechsel mit Brent/Bisektion verfeinern). Sie ist für alle Kurven- und Flächentypen einheitlich.
- Ob ein Schnittkurvenstück innerhalb **beider** beschnittener Flächen liegt, entscheidet der Punkt-in-Fläche-Test.

**Sonderfälle:**
- **Bündig:** Zwei Flächen liegen innerhalb der Toleranz auf derselben Ebene bzw. derselben Zylinder-/Kegel-/Kugel-/Torusfläche. Es gibt keine Schnittkurve; das Paar wird als „bündig“ markiert, Gleich- oder Gegenläufigkeit folgt aus dem Skalarprodukt der Außennormalen.
- **Berührung ohne Durchdringung** (z. B. Zylinder streift eine Ebene an einer Linie): doppelte Nullstelle → keine Schnittkurve. Entartete Fälle, die die Toleranz nicht sauber auflösen kann, brechen mit `AmbiguousContact` ab.

## 5. Flächen auftrennen und beschnittene Flächen speichern

**Datenmodell:** `Face` behält `surface`, `same_sense`, `outer`, `inner`. Das bisherige `patch` (`UvRect`) wird zur **Grunddomäne** der Fläche (z. B. u ∈ [0, 2π]); was davon übrig ist, bestimmen allein die Randschleifen. Schleifen bleiben 3D-Kanten. Extrusion, Rotation und Transformationen bleiben unverändert; unbeschnittene Flächen erkennt man daran, dass ihre Schleifen die kanonische Rechteckform der Domäne haben.

**Auftrennen einer Fläche:** pro Fläche, die von Schnittkurven getroffen wird, im Parameterraum:
1. Randkanten und Schnittkurvenstücke werden mit `project` in (u,v) abgebildet (bei Ebenen: Lage in der Ebene).
2. Daraus entsteht ein **ebener Graph** (Ecken, Kanten, an jeder Ecke nach Tangentenrichtung sortiert, Sortierung mit exakten Prädikaten). Die Flächenstücke sind seine geschlossenen Umläufe.
3. Außen- und Löcherschleifen werden über Enthaltensein getrennt; jede Außenschleife samt Löchern wird eine neue `Face`.

**Naht (Seam):** Zylinder, Kegel, Kugel, Torus sind in u periodisch. Die Domäne wird als Rechteck aufgerollt; linker und rechter Rand sind dieselbe 3D-Kante. Der Graph kennt diese Gleichsetzung; ein Schnitt, der die Naht kreuzt, teilt die Nahtkante an beiden Rändern gleichzeitig.

**Pole:** Bei der Kugel und der Kegelspitze fallen Rechteckränder zu einem Punkt zusammen. Der Graph fügt dafür virtuelle Poldecken ein; sie erscheinen nie als Kanten im Körper.

**Bündige Flächen:** Beide Randschleifen werden überlagert; das Überlappungsstück wird als eigenes „bündig“-Stück behandelt.

## 6. Einordnen, Auswählen, Zusammennähen

**Einordnen (`classify`):** Für jedes Flächenstück wird ein Punkt im Inneren samt Außennormale gewählt und gegen den anderen Körper geprüft:
- **Innen/außen:** Strahlverfahren. Der Strahl wird analytisch gegen alle Flächen des anderen Körpers geschnitten (Nullstellen von `F` entlang der Geraden) und mit dem Punkt-in-Fläche-Test geprüft; gerade Trefferzahl = außen, ungerade = innen. Streift der Strahl eine Kante/Ecke oder liegt er in einer Fläche, wird die nächste Richtung aus einer festen Liste genommen, danach `AmbiguousClassification`.
- **Bündig:** Der Punkt liegt innerhalb der Toleranz auf einer Fläche des anderen Körpers; gleich- oder gegenläufig nach Normalen-Skalarprodukt.

**Auswahl (`select`):**

| Stück | Vereinigen | Abziehen (A − B) | Schneiden |
|---|---|---|---|
| A außen | behalten | behalten | weg |
| A innen | weg | weg | behalten |
| B außen | behalten | weg | weg |
| B innen | weg | umdrehen, behalten | behalten |
| bündig, gleiche Richtung | eine Kopie | weg | eine Kopie |
| bündig, entgegengesetzt | weg | A behalten | weg |

(Bei bündigen Stücken gilt in der Zeile „Abziehen“ stets die Fläche von A.)

**Zusammennähen (`sew`):** Ecken innerhalb der Toleranz werden verschweißt, Kanten mit gleichen Endpunkten und gleicher Kurve zu einer. Das Ergebnis ist ein `Solid` im gewohnten Format. Kanten, die danach nur eine Fläche haben, sind ein Fehler (`NotClosed`). Bündig aneinanderliegende Flächen werden **nicht** automatisch zusammengefasst; Ergebnisse sind korrekt, aber nicht minimal (spätere Bereinigung).

**Ergebnisprüfung (immer):**
1. Validator (Topologie, Orientierung, Euler-Poincaré **je Schale**).
2. Volumenprobe über das Netz: V(Ergebnis) liegt in den erwarteten Grenzen (z. B. Vereinigen ≥ max(V_A, V_B) − Toleranz).
3. Leeres Ergebnis → `Err(EmptyResult)`; die App zeigt eine Meldung.
Schlägt eine Prüfung fehl → `ResultInvalid`.

## 7. Validator und Netz

**Validator:** zusätzlich je Fläche nicht überlappende, korrekt orientierte Schleifen; Euler-Poincaré getrennt je Schale (bisher: pro Solid).

**Netz (`planora-mesh`):**
- Unbeschnittene gekrümmte Flächen laufen unverändert über das Raster (Schnellweg, nichts Bestehendes ändert sich).
- Beschnittene gekrümmte Flächen: Randpunkte aus den Kantenpolylinien (gemeinsame Teilungsregel, damit Nachbarflächen dieselben Randpunkte haben), Innenpunkte als Raster in (u,v), eingeschränkte Delaunay-Triangulierung mit `spade`, Rückabbildung auf die Fläche, Dreiecke nach `same_sense` orientiert. Periodische Schleifen werden vor der Triangulierung entlang der Naht aufgerollt.
- Beschnittene ebene Flächen laufen wie bisher über earcut mit Löchern.

## 8. Toleranz und Robustheit

- Alles nutzt die zentrale `Tolerance`; keine neuen Epsilon-Konstanten im Boolean-Code.
- Exakte Prädikate (`robust::orient2d`) bei Orientierungsentscheidungen im Parameterraum (Graph-Aufbau, Punkt-in-Fläche).
- Jede Unklarheit liefert einen benannten Fehler; Panik ist für gültige Eingaben ausgeschlossen. Eingangskörper werden zuerst validiert.

## 9. Oberfläche (`planora-app`)

- Menü **Bearbeiten → Vereinigen / Abziehen / Schneiden** öffnet einen Dialog mit Auswahl von Körper A, Körper B und Operation. Das Ergebnis ersetzt beide Eingangskörper; der Name nennt die Herkunft (z. B. „Abzug: Quader 1 − Zylinder 2“). Bei Mehrschalen-Ergebnissen entsteht je ein Körper pro Schale.
- **Körper verschieben:** In den Eigenschaften lassen sich Position x/y/z des gewählten Körpers ändern (Translation über `Kernel::transform`). Heute werden neue Körper nebeneinander platziert; ohne Verschieben ließen sich Boolesche Operationen nicht vorführen.
- Fehler des Kernels erscheinen auf Deutsch in der Statuszeile.
- Alle UI-Texte deutsch; Icons weiterhin selbst gezeichnet.

## 10. Tests und Qualitätssicherung

- **Eigenschaftstests (proptest):** zufällige Quader und Zylinder (versetzt, in 90°-Schritten gedreht, damit Stufe A greift), je Union/Differenz/Schnitt: Ergebnis gültig, Netz dicht, Orientierung korrekt (positives Volumen) und die **Volumenbilanzen** V(A∪B) + V(A∩B) = V(A) + V(B) und V(A−B) = V(A) − V(A∩B) innerhalb der Netz-Toleranz.
- **Gezielte Fälle:** Quader − Zylinder (Durchgangs- und Sackloch), Quader ∪ Quader bündig auf einer Fläche, Zylinder ∪ Zylinder koaxial, Kugel − Quader, Zylinder in Quader mit Naht auf einer Kante, Ergebnis aus zwei getrennten Teilen, leerer Schnitt, nicht unterstütztes Paar → Fehler.
- **Kettenoperationen:** Ergebnis einer Operation ist Eingabe der nächsten (z. B. Platte − Loch − Loch).
- **Schnittfunktionen:** jedes Flächenpaar der Tabelle gegen eine numerische Gegenprobe (alle Punkte der Schnittkurve liegen auf beiden Flächen).
- **Netz:** beschnittene gekrümmte Flächen liefern geschlossene, durchgängig orientierte Netze mit richtigem Volumen.
- **CI:** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`, `cargo deny check` (Windows + Linux).

## 11. Erfolgskriterien

1. Quader − Zylinder, Quader ∪ Quader und Zylinder ∩ Quader liefern im Programm sichtbar korrekte Körper.
2. Alle Property-Tests und Fallbeispiele laufen grün, ebenso fmt, clippy und `cargo deny`.
3. Nicht unterstützte Paare und Randfälle liefern eine verständliche Meldung statt Absturz oder falschem Körper.
4. Neue Abhängigkeit ist nur `spade` (Lizenz geprüft).

## 12. Nicht im Umfang

Allgemeine Flächenschnitte (Raumkurven, schräg durchdringende Zylinder), Spline-Flächen, Rundungen/Fasen (Teilprojekt 6), automatisches Zusammenfassen bündiger Flächen, Kanten-/Ecken-Berührungsfälle jenseits des bündigen Flächenfalls, Undo/Rückgängig, Speichern/Laden.

## 13. Risiken

- **Hauptrisiko:** Robustheit des ebenen Graphen auf periodischen Flächen (Naht, Pole, Tangenten gleicher Richtung). Gegenmaßnahme: Nähte/Pole werden im Graphen explizit modelliert, Tests mit Fugen und Polen schon im Aufbau, benannte Fehler statt stiller Fehler.
- **Einordnung** in Randfällen (Strahl streift Kante): feste Ersatz-Richtungen, danach Fehler.
- **Netzqualität** beschnittener gekrümmter Flächen (dünne Dreiecke): `spade` mit Steiner-Punkten aus dem Raster; Prüfung durch Volumen- und Dichtheitstests.
- **Zeitrisiko:** das umfangreichste Teilprojekt bisher; die Aufteilung in unabhängig testbare Schritte (siehe Plan) begrenzt das.
