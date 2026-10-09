# PlanoraCAD – Spec Teilprojekt 4: Skizzen + Constraint-Solver

Datum: 2026-10-09 · Status: Entwurf zur Freigabe · Baut auf: `2026-10-09-fundament-und-kernel-design.md` (Teilprojekt 1+2) und `2026-10-09-boolesche-operationen-design.md` (Teilprojekt 3), beide umgesetzt und nach `master` gemergt.

## 1. Ziel und Einordnung

Der Nutzer zeichnet auf einer Ebene eine 2D-Skizze, hält sie mit Abhängigkeiten und Maßen fest und erzeugt aus ihren geschlossenen Profilen Körper (Extrusion, Rotation), die er mit den Booleschen Operationen weiterverarbeitet. Bisher gibt es dafür nur feste Beispielprofile.

**Umfang (vom Projekteigner gewählt: „Breit“):**
- Geometrie: Punkt, Linie, Kreis, Bogen, Ellipse, Spline (kubisch), Rechteck, Konstruktionsgeometrie, Spiegeln, Muster.
- Abhängigkeiten: koinzident, Punkt auf Objekt, horizontal, vertikal, parallel, senkrecht, tangential, gleich, fest, symmetrisch, Mittelpunkt.
- Maße: Abstand, Radius, Durchmesser, Winkel.
- Solver mit Freiheitsgrad-Anzeige, Erkennung redundanter und widersprüchlicher Abhängigkeiten.
- Körperkanten **einmalig als Konstruktionsgeometrie kopieren** (keine lebende Verknüpfung; die kommt mit dem Feature-Baum).
- Skizzen sind **eigene Objekte** neben den Körpern, jederzeit wieder bearbeitbar. Extrusion/Rotation erzeugt daraus einen Körper als **Momentaufnahme** (keine Neuberechnung bei späterer Änderung; die bringt der Feature-Baum).

**Nicht im Umfang:** Rückgängig/Wiederherstellen, Speichern/Laden, Skizzen auf gekrümmten Flächen, lebende Projektion, Körper aus Ellipsen- oder Spline-Profilen (siehe 3.4), Bemaßungsgleichungen/Parameter, Skizzen-Text.

**Erfolgskriterium:** Eine Skizze lässt sich in der App zeichnen, bemaßen und bearbeiten; der Solver löst sie reproduzierbar und meldet Freiheitsgrade, Redundanz und Widerspruch korrekt; ein geschlossenes Linien-/Bogen-Profil lässt sich extrudieren oder drehen und das Ergebnis besteht den Kernel-Validator.

### Einordnung in die Teilprojekte

| # | Teilprojekt | Stand |
|---|---|---|
| 1+2 | Fundament, Kernel-Basis | umgesetzt |
| 3 | Boolesche Operationen | umgesetzt |
| **4** | **Skizzen + Constraint-Solver** | **diese Spec** |
| 5–8+ | Feature-Baum, Fillets, Export, NURBS, … | später |

## 2. Getroffene Grundentscheidungen

- **Eigener Solver in reinem Rust, keine neue Abhängigkeit** (vom Projekteigner bestätigt). Alternativen (nalgebra, konstruktiver Solver) wurden verworfen: nalgebra spart wenig Code und verlängert die Übersetzung; ein konstruktiver Solver wird bei Schleifen und Überbestimmung unhandlich.
- **Neues Crate `planora-sketch`** (Datenmodell, Solver, Profil-Extraktion; ohne UI, ohne Grafik). Es hängt nur von `planora-math` und `planora-kernel` ab. Die Bedienung liegt in `planora-app`, die Darstellung nutzt den vorhandenen Linienrenderer in `planora-render`.
- **Alle Zahlenwerte einer Skizze liegen in einem flachen Vektor `x`.** Geometrieobjekte verweisen nur auf Indizes darin; Abhängigkeiten liefern Residuen und exakte Ableitungen über diese Indizes.
- **Toleranzen kommen aus `Tolerance`** (Abbruchschwelle, Rangentscheidung, Gleichheit). Keine neuen festen Epsilon-Werte im Code; nötige Konstanten sind benannt und begründet.
- Alle anderen Grundentscheidungen (GPL-3.0-only, Lizenzen immer prüfen, kein Autodesk-Material, UI-Texte deutsch) gelten unverändert.

## 3. Datenmodell (`planora-sketch`)

### 3.1 Skizze und IDs

`Sketch { frame: Frame, name, vars: Vec<f64>, points, curves, constraints }`. Objekte liegen in Arenen mit typisierten IDs (`Id<Point>`, `Id<Curve>`, `Id<Constraint>`), stabil über Löschen. `frame` ist die Skizzenebene (Ursprung, x, y, Normale z) und kann nachträglich nicht verschoben werden.

### 3.2 Geometrie

| Objekt | Variablen (Indizes in `x`) |
|---|---|
| Punkt | x, y |
| Linie | Anfangs- und Endpunkt (Verweise auf Punkte) |
| Kreis | Mittelpunkt (Punkt), Radius |
| Bogen | Mittelpunkt, Anfangs- und Endpunkt (Punkte), Radius; Drehsinn gegen den Uhrzeigersinn |
| Ellipse | Mittelpunkt, Vektor der großen Halbachse `(ax, ay)` (Länge = große Halbachse, Richtung = Lage), kleine Halbachse |
| Spline | n ≥ 2 Stützpunkte (Punkte); kubisch, Endbedingung „natürlich“, optional Anfangs-/Endtangente als Variablen |

Jedes Objekt trägt ein Flag `construction`. Konstruktionsgeometrie wird gelöst und angezeigt, aber nie zu Profilen zusammengesetzt. Punkte teilen sich Variablen: Zwei Linien mit gemeinsamem Endpunkt verweisen auf denselben `Id<Point>` (das ist die feste Form von „koinzident“; die Abhängigkeit „koinzident“ gleicht zusätzlich zwei verschiedene Punkte an).

Rechteck, Spiegeln und Muster sind **keine eigenen Typen**. Sie sind Hilfen, die Linien/Punkte samt Abhängigkeiten erzeugen (Rechteck: 4 Linien, koinzident, horizontal/vertikal; Spiegeln: gespiegelte Kopie + Symmetrie-Abhängigkeiten zur Achse; Muster: Kopien + Abstands-/Winkel-Abhängigkeiten).

### 3.3 Abhängigkeiten und Maße

Jede Abhängigkeit liefert `k` Residuen `r(x)` und deren Ableitungen:

| Abhängigkeit | Residuen |
|---|---|
| koinzident (P, Q) | 2 (`Px−Qx`, `Py−Qy`) |
| Punkt auf Linie/Kreis/Bogen/Ellipse/Spline | 1 (Abstand zur Kurve; bei Spline mit Zusatzvariable Kurvenparameter) |
| horizontal / vertikal (Linie oder zwei Punkte) | 1 |
| parallel / senkrecht (zwei Linien) | 1 (Kreuz- bzw. Skalarprodukt der Richtungen, normiert) |
| tangential (Linie–Kreis/Bogen, Kreis–Kreis, Kurve–Kurve) | 1 (Abstandsbedingung) + 1 (Berührung); Lage gleich-/gegenläufig wird beim Anlegen aus der aktuellen Geometrie gewählt und gespeichert |
| gleich (Länge/Radius) | 1 |
| fest (Punkt oder Objekt) | 2 je Punkt |
| symmetrisch (zwei Punkte zu Linie/Achse) | 2 |
| Mittelpunkt (Punkt auf der Mitte einer Linie) | 2 |
| Maß Abstand (zwei Punkte / Punkt–Linie) | 1 (`dist − d`) |
| Maß Radius / Durchmesser | 1 |
| Maß Winkel (zwei Linien) | 1 über Richtungsvektoren (kein `atan2`, keine Sprünge bei ±π) |

Maße sind Abhängigkeiten mit Zahlenwert (`value`) und Anzeigeposition. Jede Abhängigkeit hat ein Flag `driven` (nur anzeigen, nicht erzwingen), damit Maße ohne Zwang angezeigt werden können.

### 3.4 Profil-Extraktion und Grenze zum Kernel

`Sketch::profiles(tol) -> Result<Vec<Profile>, SketchError>` bildet aus den Nicht-Konstruktionsobjekten geschlossene Schleifen (Ketten über gemeinsame Endpunkte, mit Toleranz `tol.linear`), erkennt Schachtelung (äußere Schleife und Löcher, auch mehrere Löcher) und liefert die bestehenden Kernel-Typen `Profile`/`ProfileLoop` aus Linien (`Segment::Line`) und Kreisbögen (`Segment::Arc`).

- Kreise werden Vollkreis-Schleifen (`ProfileLoop::circle`).
- Enthält eine Schleife eine **Ellipse oder einen Spline**, schlägt `profiles()` für diese Schleife mit `SketchError::UnsupportedForSolid("Ellipse"|"Spline")` fehl („Dieses Profil enthält … und kann noch nicht zu einem Körper werden“). Der Kernel kennt dafür noch keine Flächen (elliptischer Zylinder, NURBS); sie kommen mit späteren Teilprojekten. Die Skizze selbst bleibt voll nutzbar.
- Sich kreuzende, überlappende oder offene Schleifen: benannte Fehler (`OpenLoop`, `SelfIntersecting`, `AmbiguousNesting`); es wird kein Profil geraten.

### 3.5 Kernel-Anpassungen (aus `offene-punkte.md`, „Vor Teilprojekt 4“)

Diese gehören ausdrücklich zu diesem Teilprojekt, weil Skizzen die öffentliche Profil-API erstmals mit nicht vertrauenswürdigen Eingaben füttern:

1. `ProfileLoop::polygon` gibt `Result` zurück (statt zu paniken bei < 3 Punkten).
2. `ProfileLoop::sample`/`signed_area` werden abgesichert (endliche Werte, begrenzte Schrittzahl, kein Hängen bei riesigen Winkeln).
3. `extrude`/`revolve` prüfen `Frame`-Eingaben (endlich, orthonormal innerhalb Toleranz).
4. Profile mit sich schneidenden oder außerhalb liegenden Löchern werden abgewiesen (`KernelError`).
5. Die Oberfläche liest Körpereigenschaften über eine Kernel-Fassade statt über `solid.faces[..]`.

Punkte 1–4 bekommen Tests, die vorher rot sind.

## 4. Solver

### 4.1 Problem und Zerlegung

Gesucht ist `x` mit `r(x) = 0`; `J = ∂r/∂x` ist dünn besetzt und wird dicht pro Gruppe aufgebaut. Variablen und Abhängigkeiten bilden einen Graphen. **Zusammenhängende Gruppen werden unabhängig gelöst.** Ändert sich nur eine Gruppe (Ziehen), rechnet nur sie neu. Typische Gruppen haben < 300 Variablen.

### 4.2 Verfahren

Newton mit Levenberg-Marquardt-Dämpfung: `(JᵀJ + λI)Δ = −Jᵀr`. `λ` wird erhöht, wenn der Fehler nicht sinkt, und gesenkt, wenn er sinkt. Die Systemmatrix wird mit **eigener dichter Cholesky-Zerlegung** gelöst; scheitert sie (nicht positiv definit), fällt das Verfahren auf **QR mit Spaltenpivotierung** zurück. Beides wird im Crate selbst geschrieben, mit eigenen Tests gegen bekannte Lösungen.

Abbruch: `‖r‖∞ ≤ Schwelle` (aus `Tolerance.linear` abgeleitet, Winkel aus `Tolerance.angular`), höchstens 100 Schritte, oder Stillstand (Schrittweite unter Maschinengenauigkeit). Ergebnis je Gruppe: `Solved`, `NotConverged`, `Conflicting`.

### 4.3 Freiheitsgrade und Diagnose

- **Freiheitsgrade** = Variablen − Rang(`J`) an der Lösung; der Rang kommt aus rangaufdeckender QR. Angezeigt gesamt („vollständig bestimmt“, „unterbestimmt, n Freiheitsgrade“) und je Objekt (welche Objekte noch frei beweglich sind, ergibt sich aus dem Nullraum von `J`).
- **Redundant**, aber widerspruchsfrei (z. B. dritter Winkel im Dreieck): `r = 0` ist lösbar, der Rang ist kleiner als die Zahl der Residuen. Die betroffenen Abhängigkeiten werden gemeldet und gelten als redundant, die Skizze bleibt lösbar.
- **Widerspruch** (`r ≠ 0` an der besten Lösung, kleinster Rest über der Schwelle): Eine kleine Gruppe in Konflikt stehender Abhängigkeiten wird über die Residuen im Bild des Restraums bestimmt und gemeldet. Die Skizze bleibt im **letzten gültigen Zustand** (kein halbes Ergebnis).

### 4.4 Ziehen und Stabilität

Ein gezogener Punkt geht als **weiche Zusatzabhängigkeit** mit kleinem Gewicht ein; der Solver bewegt die Skizze minimal. Der Solver startet immer vom letzten Zustand und bevorzugt die nächstgelegene Lösung, damit mehrdeutige Lagen (Bogen oben/unten, tangential gleich-/gegenläufig) nicht beim Ziehen umklappen. Die gewählte Tangentiallage wird je Abhängigkeit gespeichert.

### 4.5 Besonderheiten

- Winkel und Bögen verwenden Richtungsvektoren (kein `atan2`).
- Splines: Stützpunkte sind Variablen; „Punkt auf Spline“ und „tangential“ nutzen Kurve und Ableitung nach dem Parameter, der Parameter der Berührstelle ist eine Zusatzvariable.
- Degenerierte Zustände (Linie der Länge 0, Radius 0): benannte Fehler, keine NaN-Ausbreitung; alle Eingaben werden auf Endlichkeit geprüft.

## 5. Oberfläche (`planora-app`)

- **Anlegen:** Menü „Erzeugen → Skizze“: Ebene XY/XZ/YZ oder planare Körperfläche anklicken. Danach **Skizzen-Modus**: Kamera schaut senkrecht auf die Ebene, Körper erscheinen abgedimmt.
- **Werkzeugleiste im Viewport:** Auswahl, Punkt, Linie, Rechteck, Kreis, Bogen, Ellipse, Spline, Spiegeln, Muster, Konstruktions-Umschalter, Körperkante kopieren, Maß.
- **Zeichnen:** Einrasten auf Punkte, Mitten, Linien, Achsen; dabei werden Abhängigkeiten angelegt (Fangpunkt auf Punkt → koinzident; horizontal/vertikal werden vorgeschlagen und sind abwählbar).
- **Abhängigkeiten/Maße:** Anlegen über Leiste oder Kontextmenü auf der Auswahl, Maß per Auswahl + Zahl + Enter; Liste im Eigenschaftenfenster (ändern, löschen).
- **Zustandsanzeige:** Freiheitsgrade in der Statuszeile; vollständig bestimmte Objekte in anderer Farbe; redundante/widersprüchliche Abhängigkeiten rot.
- **Beenden/Wieder öffnen:** „Skizze beenden“ legt sie als Objekt im Browser ab; Doppelklick öffnet sie wieder.
- **Zum Körper:** Rechtsklick auf Skizze oder Profil → „Extrudieren …“/„Drehen …“ öffnet die vorhandenen Dialoge mit dem gewählten Profil. Ergebnis: normaler Körper (Momentaufnahme).
- **Körperkante kopieren:** Wähle Kante(n) eines Körpers; ihre Projektion auf die Skizzenebene wird als Konstruktionsgeometrie (Linien, Bögen, Kreise; Ellipsen als Ellipse) eingefügt, ohne Verknüpfung.
- **Code-Trennung:** Der Skizzen-Zustand (Werkzeug, Auswahl, Vorschau, Ziehen) liegt in `planora-app::sketch_state` ohne egui-Typen und ist ohne Fenster testbar; egui-Code zeichnet nur und leitet Eingaben weiter. `AppState` bekommt `sketches: Vec<SketchObject>`.
- Alle Texte deutsch; keine Autodesk-Namen, -Symbole oder -Begriffe als Marken.

## 6. Fehlerbehandlung

- Der Solver ändert die Skizze nur bei `Solved`/redundant-lösbar; bei `Conflicting` und `NotConverged` bleibt der letzte gültige Zustand, die Oberfläche zeigt die Ursache (welche Abhängigkeiten).
- `SketchError` (thiserror, deutsche Meldungen): `OpenLoop`, `SelfIntersecting`, `AmbiguousNesting`, `UnsupportedForSolid(..)`, `InvalidInput`, `Degenerate`.
- Jede Aktion in der Oberfläche, die fehlschlägt, lässt das Dokument unverändert und zeigt die Meldung in der Statuszeile.
- Keine Panics auf Nutzereingaben: Indizes und Zahlen werden an den API-Grenzen geprüft.

## 7. Tests

- **Ableitungen:** Für jede Abhängigkeit prüft ein Test die analytische Jacobi-Matrix gegen Differenzenquotienten an Zufallspunkten.
- **Lineare Algebra:** Cholesky und QR gegen bekannte Lösungen, singuläre und schlecht konditionierte Matrizen, Rangbestimmung.
- **Solver, bekannte Skizzen:** Rechteck mit Maßen, Dreieck (Redundanz), Kreis mit Tangenten, Langloch, Schlüsselloch, symmetrische Skizzen; erwartete Freiheitsgrade und Lösungen.
- **Eigenschaft (proptest, feste Seeds):** Skizzen werden aus einer bekannten Lösung gebaut (Zufallsgeometrie, daraus abgeleitete Abhängigkeiten), Startwerte gestört; der Solver muss zu einer Lösung finden, die alle Abhängigkeiten erfüllt. Mindestzahl geprüfter Fälle wird erzwungen (keine leeren Läufe).
- **Widerspruch/Redundanz:** gezielte Skizzen mit zwei widersprüchlichen Maßen bzw. einer redundanten Abhängigkeit; Meldung nennt die beteiligten Abhängigkeiten.
- **Ziehen:** Skizze bleibt stabil (kein Umklappen), Lösung ist die nächstgelegene.
- **Profil-Extraktion:** Rechtecke, Kreise, Schlüsselloch, Löcher, Schachtelung, offene Schleifen, Selbstschnitt, Ellipse/Spline → `UnsupportedForSolid`.
- **Ende zu Ende:** Skizze → Profil → `extrude`/`revolve` → Validator, Volumen stimmt; danach Boolesche Operation mit dem Ergebnis.
- **Kernel-Anpassungen:** je ein roter Test für die vier Punkte aus 3.5.
- **App:** Zustand ohne Fenster (Zeichnen, Auswahl, Abhängigkeit setzen, Skizze beenden/öffnen, Extrudieren); egui-Rauchtests ohne Fenster wie bei den Booleschen Operationen.

## 8. Risiken

| Risiko | Gegenmaßnahme |
|---|---|
| Mehrdeutige Lösungen (Umklappen) | Start vom letzten Zustand, nächstgelegene Lösung, gespeicherte Tangentiallage |
| Konvergenz bei schlechtem Start | Levenberg-Marquardt mit Dämpfung, Rückfall auf QR, Zufallstests mit gestörtem Start |
| Rangentscheidung bei Rauschen (Freiheitsgrade) | Schwelle aus `Tolerance`, relativ zur größten Singulärgröße; Tests an Grenzfällen |
| Spline-Tangential/Punkt-auf ist numerisch heikel | Eigene Testreihe, Parameter als Zusatzvariable, bei Nichtkonvergenz klarer Fehler statt Raten |
| Umfang „Breit“ ist groß | Implementierungsplan in klar getrennten Aufgaben (Algebra → Solver → Geometrie/Abhängigkeiten → Profile → Kernel-Anpassungen → App); Splines/Ellipse als späte, abtrennbare Aufgaben |
| Bedienung im Viewport ist umfangreich | Zustand von egui trennen, testbar halten; Funktionsumfang pro Werkzeug klein halten |

## 9. Lizenzen und Abhängigkeiten

Keine neue externe Abhängigkeit geplant (eigener Solver). Falls eine im Plan doch nötig wird: vorher Lizenz prüfen, `cargo deny check` bleibt grün, `deny.toml` nie ohne Rückfrage erweitern. Kein Autodesk-Material; die Werkzeug- und Abhängigkeitsnamen sind allgemeine CAD-Begriffe.
