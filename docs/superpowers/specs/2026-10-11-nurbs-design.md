# NURBS im Kernel – Entwurf (Teilprojekt 7)

Stand: 2026-10-11. Baut auf den Specs zu Fundament/Kernel, Booleschen Operationen, Skizzen, Feature-Baum und Speichern auf. Das Teilprojekt 8 (Rundungen und Fasen mit konstantem und variablem Radius sowie Ecken-Stilen) setzt darauf auf; es ist der Grund für dieses Teilprojekt.

## 1. Ziel und Umfang

Der Kernel kann heute nur analytische Flächen (Ebene, Zylinder, Kegel, Kugel, Torus). Dieses Teilprojekt fügt **freie Flächen und Kurven (NURBS)** hinzu und macht sie in der ganzen Kette nutzbar:

- NURBS-Rechnung als eigener, getesteter Baustein,
- `Curve::Nurbs` und `Surface::Nurbs` im Kernel mit Rückrechnung auf Parameter, nächstem Punkt, Versatzfläche, Netz, Volumen und Prüfung,
- Ellipsen und Splines aus Skizzen werden zu Körpern (Extrusion, Rotation),
- numerische Schnitte zwischen NURBS-Flächen und allen anderen Flächen sowie die Booleschen Operationen darauf,
- Dateiformat 2 (Format 1 bleibt lesbar).

**Nicht Teil dieses Teilprojekts:** Loft und Sweep (eigenes späteres Teilprojekt), Rundungen/Fasen (Teilprojekt 8), Bearbeiten von Kontrollpunkten durch den Nutzer, Import/Export von Fremdformaten, Schnitte mit Selbstdurchdringung (werden erkannt und gemeldet, nicht berechnet).

## 2. Entscheidungen

| Nr. | Entscheidung | Begründung |
|---|---|---|
| N1 | Allgemeine rationale B-Splines (NURBS) als neue Enum-Varianten neben den analytischen Arten; beliebiger Grad, Tensorprodukt-Flächen | Kreise und Ellipsen exakt darstellbar; nichts Bestehendes ändert sich |
| N2 | Alles selbst gerechnet in `planora-math`, keine neue externe Abhängigkeit | Lizenzregel; die Rechnung ist überschaubar und gut testbar |
| N3 | Numerische Schnitte per Verfolgen der Schnittkurve (Startpunkte durch Unterteilung, Newton-Nachbesserung, Schrittweite nach Krümmung), danach Annäherung als NURBS-Kurve mit fester Schranke | Allgemein genug für beliebige Paare; algebraische Wege gibt es nur für Sonderfälle |
| N4 | Zwei Toleranzen: analytisch weiter 1e-6, Annäherung von Schnittkurven 1e-5 mm; Prüfungen nehmen für NURBS-Kanten die größere | Ehrlich zur Numerik; zentral benannt, nicht verstreut |
| N5 | Alle bisherigen analytischen Wege bleiben unverändert; numerische Wege nur, wenn mindestens eine Seite NURBS ist | Bestehende Tests und Verhalten bleiben gültig |
| N6 | Nicht lösbare Fälle werden verständlich gemeldet (deutsch, mit Featurename), nicht beschönigt; das Dokument bleibt unverändert | Wie bei den bisherigen Booleschen Operationen |
| N7 | Fester Satz benannter Grenzen (Unterteilungstiefe, Zahl der Startpunkte, Kurvenlänge, Schrittzahl) | Eine Rechnung läuft nie unbegrenzt; Verhalten ist reproduzierbar |
| N8 | Ein Teilprojekt, in Stufen geschnitten (§9), jede Stufe mit eigenem Abschlussreview | Nutzerentscheidung; Risiko wird durch Stufen beherrschbar |

## 3. NURBS-Rechnung (`planora-math`)

- Typen für Knotenvektor, NURBS-Kurve und NURBS-Fläche (Kontrollpunkte als homogene `DVec4` oder Punkt plus Gewicht), jeweils mit **geprüftem Konstruktor** (`Result`): Grad ≥ 1, Knoten nichtfallend und endlich, Knotenzahl = Kontrollpunkte + Grad + 1, Gewichte endlich und > 0, Kontrollpunkte endlich und innerhalb von `MAX_COORD` (1e9 mm), Kontrollpunktzahlen begrenzt.
- Auswerten von Punkt, erster und zweiter Ableitung (Cox/de Boor, rational), Parameterbereich, Gradanhebung, Knoteneinfügen, Teilen an einem Parameter, Umkehr der Richtung.
- Exakte Darstellungen: Kreisbogen und Ellipse (rationale quadratische Segmente), Rotationsfläche einer beliebigen NURBS-Profilkurve um eine Achse (volle Drehung als neun Kontrollpunkte je Querschnitt, Teildrehungen entsprechend), Verschiebefläche (Extrusion) einer Kurve entlang einer Richtung.
- Natürlicher kubischer Spline durch Punkte als B-Spline-Kurve (Übereinstimmung mit der Skizze auf 1e-9).
- Annäherung einer Punktfolge als Kurve mit Fehlerschranke (Interpolation mit Sehnenlängenparametrisierung, Verfeinerung bis die Schranke gilt).

## 4. Kernel-Typen (`planora-kernel`)

- `Curve::Nurbs(Arc<NurbsCurve>)`, `Surface::Nurbs(Arc<NurbsSurface>)`. `Arc`, damit das Kopieren von Körpern billig bleibt.
- Natürliche Normale `∂P/∂u × ∂P/∂v`, Orientierung über `same_sense` wie bisher.
- **Periodische Flächen:** ein Kennzeichen „geschlossen in u/v“ (Naht wie beim Zylinder); die Umrechnung von 3D-Punkten auf Parameter entfaltet den Parameter fortlaufend.
- `closest_uv(p, hint)`: grobe Rastersuche (Startwert aus dem Vorgänger, sonst aus dem Raster), Newton-Nachbesserung; liefert auch den Abstand. Scheitert die Suche, ist das ein Fehler, kein Raten.
- `offset(d)`: Versatzfläche als NURBS-Näherung mit Fehlerschranke (für Teilprojekt 8); Abbruch mit Fehler bei Krümmungsradius kleiner als `|d|` (Selbstüberlappung).
- Netz (`planora-mesh`): adaptiv nach Sehnentoleranz, Obergrenze der Dreieckszahl wie bisher; beschnittene NURBS-Flächen über die vorhandene eingeschränkte Delaunay-Triangulierung im Parameterraum.
- Prüfung (`validate`): Kanten-Flächen-Abstand nimmt für NURBS-Kanten die größere Toleranz (N4); alles Weitere unverändert.
- Volumen und Schwerpunkt über das Netz wie bisher.

## 5. Skizzen-Körper (`planora-sketch`, `planora-kernel`)

- Ellipse und Spline im Profil werden zugelassen; die Meldung „kann noch nicht zu einem Körper werden“ entfällt für diese Arten.
- Extrusion: Gerade → Ebene, Kreis → Zylinder (wie heute), Ellipse und Spline → NURBS-Fläche vom Grad `(Kurvengrad, 1)`.
- Rotation: Gerade und Kreisbogen bleiben analytisch (wie heute), Ellipse und Spline → exakte rationale Drehfläche.
- Rollen der Flächen (`Boden`, `Deckel`, `Wand k`, `Fläche k`) und die Herkunftsmarken bleiben; das Feature im Baum ändert sich für den Nutzer nicht.
- Profile mit Ellipse/Spline werden wie bisher auf Selbstüberschneidung und Verschachtelung geprüft; Fehler wie heute.

## 6. Schnitte und Boolesche Operationen (`planora-kernel::boolean`)

Pipeline wie bisher (Schnittkurven, Flächen teilen, Teile klassifizieren, auswählen, vernähen). Neu:

1. **Paare:** Ebene–NURBS, Zylinder/Kegel/Kugel/Torus–NURBS, NURBS–NURBS.
2. **Startpunkte:** Beide Flächen werden rekursiv unterteilt, bis die Teilstücke nahezu eben sind; überlappende Hüllquader ergeben Kandidaten; Newton-Nachbesserung liefert Punkte auf beiden Flächen.
3. **Verfolgen:** in beide Richtungen mit Schrittweite nach Krümmung und Sehnentoleranz; jeder Punkt wird auf beide Flächen zurückprojiziert; Ende am Parameterrand, bei Rückkehr zum Anfang (geschlossene Kurve) oder beim Treffen auf eine andere verfolgte Kurve.
4. **Ordnen:** doppelt gefundene Kurven zusammenführen, Schleifen schließen, Verzweigungen erkennen.
5. **Annähern:** je Schnittkurve eine NURBS-Kurve mit Fehlerschranke 1e-5 mm (N4); die Kante im Körper trägt sie.
6. **Parameterkurven:** Rückrechnung der Kurvenpunkte auf jede Fläche (Newton, Vorgänger als Startwert, Nähte fortlaufend entfaltet).
7. **Klassifizieren:** Teststrahl wie bisher; neu Strahl–NURBS-Schnitt (Unterteilung plus Newton) mit Erkennung von Streifschüssen und Wahl eines anderen Teststrahls.
8. **Grenzen und Meldungen (N6/N7):** Berührung nur tangential, zusammenfallende Flächen aus verschiedenen NURBS-Daten, Selbstdurchdringung, nicht zusammenhängend verfolgbare Kurven, überschrittene Grenzen → verständliche deutsche Meldung mit Featurename; kein Teilergebnis.
9. Jedes Ergebnis durchläuft `validate` und die Volumenprobe des Feature-Baums; bei Misserfolg Fehlerstatus am Feature, Dokument unverändert.

## 7. Dateiformat 2 (`planora-project`)

- Format 2 ergänzt die Kurvenart `nurbs` (Grad, Knoten, Gewichte, Kontrollpunkte). Die Zeitleiste enthält weiterhin nur Skizzen und Parameter; freie Flächen werden nicht gespeichert, sondern beim Laden aus der Zeitleiste neu berechnet.
- **Skizzen-Splines** werden bereits als Punkte gespeichert (Format 1); das bleibt, es gibt keine neue Notwendigkeit, NURBS in Skizzen zu speichern. Format 2 wird daher **nur** gebraucht, falls Teilprojekt 7 neue Zeitleisten-Parameter einführt; ergibt die Umsetzung, dass das nicht nötig ist, bleibt es bei Format 1 und dies wird am Ende der Umsetzung in einem Abschnitt „Änderungen bei der Umsetzung“ festgehalten.
- Beim Laden werden alle NURBS-Daten wie jede Eingabe geprüft (geprüfte Konstruktoren, Grenzen), nie ohne Prüfung übernommen.

## 8. Oberfläche

Keine neuen Bedienelemente. Der Nutzer sieht: Ellipse und Spline lassen sich extrudieren und drehen; Boolesche Operationen mit solchen Körpern funktionieren; bei nicht lösbaren Fällen erscheinen verständliche Fehlermeldungen am Feature im Baum (wie bei den bisherigen Grenzen).

## 9. Stufen (jede einzeln grün, mit eigenem Abschlussreview)

1. NURBS-Rechnung in `planora-math`.
2. Kernel-Typen: `Curve::Nurbs`, `Surface::Nurbs`, Rückrechnung, nächster Punkt, Versatzfläche.
3. Netz, Volumen, Prüfung mit NURBS-Flächen.
4. Skizzen-Körper: Extrusion und Rotation von Ellipse und Spline.
5. Schnitte: Ebene–NURBS, dann analytische Flächen–NURBS, dann NURBS–NURBS.
6. Klassifizieren und Vernähen für gemischte Körper, Boolesche Operationen in der App.
7. Dateiformat (falls nötig), Golden-Dateien, Dokumentation, Gesamtprüfung, Handprüfung.

## 10. Tests

- **Rechnung:** Kreis und Kugel als NURBS gegen die analytische Fläche (Abweichung < 1e-12); Teilung der Einheit; Knoteneinfügen und Gradanhebung ändern die Kurve nicht (proptest); Ableitungen gegen Differenzenquotient.
- **Körper:** extrudiertes Ellipsenprofil: Volumen gegen `π·a·b·h`; Rotationskörper gegen bekannte Volumen; Spline-Extrusion gegen feine Referenzintegration.
- **Schnitte:** NURBS-Kugel gegen Ebene ergibt einen Kreis; Punkte der Schnittkurve liegen auf beiden Flächen (Abstand < 1e-5); geschlossene Körper nach dem Vernähen (`validate`); Volumen: Vereinigen + Schneiden = Summe der Einzelvolumen innerhalb der Toleranz; Streifschuss- und Berührfälle liefern die benannte Meldung statt eines falschen Ergebnisses.
- **Feindliche Eingaben:** Gewicht ≤ 0, falscher Knotenvektor, NaN/Inf, riesige Kontrollpunkte, Grad 0, zu viele Kontrollpunkte, zufällig verfälschte Daten: Fehler statt Absturz, auch beim Laden.
- **App:** headless-Tests für Extrusion/Rotation von Ellipse/Spline und die Booleschen Dialoge mit solchen Körpern; Handprüfung mit `tools/gui-drive.ps1` (nur in Absprache mit dem Nutzer, der dabei Maus und Tastatur nicht anfasst).
- Alle bisherigen Tests (718) bleiben unverändert grün.

## 11. Risiken

- Numerische Schnitte sind der riskanteste Teil (Streifschüsse, Berührungen, enge Kurven, Singularitäten). Ziel ist „funktioniert für CAD-typische Fälle zuverlässig, sonst klare Meldung“, nicht Vollständigkeit. Wird dieses Ziel in Stufe 5 oder 6 verfehlt, wird das offen berichtet.
- Zwei Toleranzen (N4): die Prüfungen müssen die größere Schranke für NURBS-Kanten kennen; zentrale Benennung, damit sie nicht verstreut wird.
- Umfang: größtes Teilprojekt bisher; Stufen und Zwischenreviews sind die Gegenmaßnahme.

## 12. Abhängigkeiten und Lizenzen

Keine neue externe Abhängigkeit geplant (`spade`, `earcutr`, `robust` sind schon da). Sollte sich unterwegs ein Paket als sinnvoll erweisen, wird zuerst `cargo deny check` ausgeführt; liegt ein Paket außerhalb der Erlaubnisliste, wird angehalten und der Eigner gefragt; die Liste wird nie selbstständig erweitert. Projektlizenz GPL-3.0-only; kein Fremdmaterial.

## 13. Änderungen bei der Umsetzung

- **Rückrechnung bei misslungener Suche (Stufe 2, Task 4):** §4 verlangt „Scheitert die Suche, ist das ein Fehler, kein Raten.“ Umgesetzt: `NurbsSurfaceData::closest(p, hint)` liefert `Option` und meldet so das Scheitern (`None`); wer es unterscheiden muss, nimmt `closest`. `Surface::project`/`project_near` (gemeinsame Schnittstelle aller Flächenarten, ohne Fehlerrückgabe) fallen dann auf den **nächsten Rasterpunkt** zurück – einen echten Flächenpunkt, nicht die Bereichsmitte. Aufrufer, die einen Punkt auf der Fläche erwarten, prüfen den Abstand (`implicit` bzw. `edge_surface_tol`).
- **`project_near` entfaltet nicht:** das Ergebnis liegt im Parameterbereich (geschlossene Richtungen gefaltet); das fortlaufende Entfalten an der Naht macht der Aufrufer (Task 9).
- **Vorzeichenbehafteter Abstand am Rand beschränkter Flächen:** Für ein beschränktes NURBS-Flächenstück springt `implicit` jenseits des Rands das Vorzeichen, ohne null zu werden. `curve_surface_roots` nimmt bei Freiformbeteiligten eine halbierte Stelle deshalb nur an, wenn dort `|implicit|` innerhalb der Toleranz liegt.
- **Netz unbeschnittener Freiformflächen (Stufe 3, Task 5):** Auch unbeschnittene NURBS-Flächen werden über die eingeschränkte Delaunay-Triangulierung im Parameterraum vernetzt (Rand = gemeinsame Kantenpunkte, innen ein Raster aus `Discretizer::nurbs_grid`), nicht als reines `nu × nv`-Raster. Grund: Die Randkanten sind oft analytisch (Kreise eines Zylinders), ihre Teilpunkte liegen nicht auf Parameterlinien der NURBS-Fläche (u = Winkel nur an den Segmentgrenzen); ein reines Raster wäre nicht dicht an die Nachbarflächen angeschlossen. Die Teilung (`nurbs_grid`) folgt dem Plan (Bézier-Abschnitte · Grad, Knickwinkel, mindestens 2 · Abschnitte, höchstens 512, bis zu drei Verfeinerungsrunden nach der Sehnentoleranz); zusätzlich wird die Zellmitte gegen den Mittelwert der vier Ecken geprüft (Verwindung, Diagonalen der Triangulierung).
- **Winkelschritt bei Freiformkörpern:** Als Radiusmaß einer Freiformkurve oder -fläche für den gemeinsamen Winkelschritt dient die volle Diagonale des Kontrollpunkt-Hüllquaders (vorher die halbe). Körper mit Freiformflächen werden dadurch feiner vernetzt als die Sehnentoleranz allein verlangt; die Volumenschranken der Tests (z. B. 0,2 % beim NURBS-Zylinder mit Sehnentoleranz 0,01) wären mit der halben Diagonale nicht erreichbar (das analytische Gegenstück hätte bei gleicher Sehnentoleranz 0,4 % Volumenfehler). Das ist eine bewusste Regel für die Volumengenauigkeit: Die Volumenschranken sind strenger als die Sehnentoleranz allein; sie wird nicht hergeleitet, sondern gilt ausdrücklich, bis ein Krümmungsmaß für Freiformflächen vorliegt.
- **Prüfung (`validate`):** Neu ist eine geometrische Prüfung „Kante liegt auf Fläche“ (`ValidationError::EdgeOffSurface`), aber nur, wenn die Kante eine Freiformkurve ist oder die Fläche eine Freiformfläche (Toleranz `edge_surface_tol`, also `NURBS_EDGE_TOL`); rein analytische Körper werden wie bisher nur topologisch geprüft.
- **Perioden:** `Surface::u_period()`/`v_period()` (analytisch `TAU`, NURBS die Bereichsbreite bei Kennzeichen „geschlossen“). Randschleifen im Parameterraum (`points_to_uv`), Punkt-in-Fläche, `classify::shifts` und das Ausrichten von Löchern im Netz nutzen sie statt fester `TAU`-Verschiebungen; für NURBS werden Randpunkte mit `project_near` und dem vorigen Punkt als Startwert zurückgerechnet (bei Abstand über `NURBS_EDGE_TOL` globale Suche).
- **Netz beschnittener Freiformflächen (Nachbesserung Task 5):** Die Rasterzellen im gestreckten Parameterraum haben ein Seitenverhältnis von höchstens 1,5 (eine Regelfläche hätte sonst nur eine Innenzeile, die Delaunay-Triangulierung bildet an Beschnitten lange, schräge Dreiecke). Danach wird die Triangulierung verfeinert, bis jede Innenkante (Kantenmitte) und jeder Dreiecksschwerpunkt höchstens um die Sehnentoleranz von der Fläche abweicht (Randkanten bleiben, höchstens 24 Runden und `MAX_GRID_POINTS`). Lässt sich ein Randpunkt nicht auf die Freiformfläche zurückrechnen (Abstand über `NURBS_EDGE_TOL`), meldet `tessellate` den Fehler `MeshError::OffSurface` statt ein offenes Netz zu liefern.
- **Parameterauflösung im nächsten Punkt (Nachbesserung Task 5):** Die Newton-Suche auf Freiformflächen endet bei einem Schritt unter `max(1e-12 · Bereich, 4 · ε · |Parameter|)` und nimmt den Rest durch die Parameterauflösung beim Gradienten hin; Bereiche fern von 0 (etwa `[1e5, 1e5 + 1]`) konvergieren damit.
