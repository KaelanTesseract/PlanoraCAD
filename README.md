# PlanoraCAD

Ein quelloffenes, parametrisches 3D-CAD in reinem Rust für Windows und Linux.

**Status:** frühe Entwicklung (Fundament, Kernel-Basis, Boolesche Operationen, Skizzen mit Constraint-Solver).

Erste Version: Quader, Zylinder, Kugel, Kegel, Extrusion und Rotation von Beispielprofilen, 3D-Ansicht mit Auswahl, Körper verschieben (Feld „Position“) sowie Boolesche Operationen (Vereinigen, Abziehen, Schneiden) für Körper mit Flächen aus Ebenen, Zylindern, Kegeln, Kugeln und Tori (Stufe A).

Bekannte Grenzen: Schnitte, die Raumkurven ergeben (z. B. zwei Zylinder mit gekreuzten Achsen), werden noch nicht unterstützt; das Vereinigen von Körpern, die sich nur an einer Kante berühren, manche genau tangentialen Lagen und Kanten oder Ecken, die genau auf einer Fläche des anderen Körpers aufliegen, liefern eine Fehlermeldung statt eines Ergebnisses (bei Berührung nur an einer Ecke entsteht ein Körper aus zwei Teilen mit gemeinsamer Ecke); es gibt noch kein Rückgängig. Details: `docs/superpowers/offene-punkte.md`.

## Skizzen

Über „Erzeugen → Skizze auf XY-/XZ-/YZ-Ebene“ oder „Skizze auf gewählter Fläche“ (ebene Fläche eines Körpers) entsteht eine Skizze. Im Skizzen-Modus schaut die Kamera senkrecht auf die Ebene; ein Fenster „Skizze“ enthält Werkzeuge, Abhängigkeiten und deren Liste.

**Was geht**
- Zeichnen: Punkt, Linie, Rechteck, Kreis, Bogen, Ellipse, Spline; Konstruktionsgeometrie über „Konstruktion umschalten“; Spiegeln und lineares Muster.
- Abhängigkeiten: deckungsgleich, waagerecht, senkrecht, parallel, rechtwinklig, tangential, gleich, fest, Mittelpunkt, symmetrisch, Punkt auf Kurve; Maße für Abstand, Abstand Punkt–Linie, Radius, Durchmesser und Winkel (Eingabe als Zahl).
- Der eigene Solver (keine neue Bibliothek) zeigt die Freiheitsgrade an, färbt bestimmte Geometrie anders ein und meldet überzählige und widersprüchliche Abhängigkeiten. Der Solver arbeitet je zusammenhängender Gruppe: Eine Gruppe mit Widerspruch (oder die sich nicht lösen lässt) bleibt in ihrem letzten Zustand, die übrigen Gruppen werden trotzdem gelöst. Lösungen, bei denen Geometrie entartet (Radius null oder negativ, Linie der Länge null, Bogenanfang auf der Mitte), gelten nicht als Lösung; die Gruppe bleibt dann ebenfalls stehen, und die Ursache wird genannt. Eine neue Abhängigkeit, die eine gelöste Skizze kaputt machen würde, wird abgelehnt; in einer schon fehlerhaften Skizze wird sie mit Warnung behalten. Ziehen startet vom letzten Zustand, mehrdeutige Lagen klappen in der Regel nicht um.
- Körperkanten einmalig als feste Konstruktionsgeometrie in die Skizze kopieren (Geraden; Kreise und Bögen in parallelen Ebenen; ganze Kreise und Ellipsen auch schräg, dann als Ellipse).
- „Skizze beenden“ legt die Skizze im Browser ab; sie bleibt als dünne Linien in der 3D-Ansicht sichtbar und lässt sich per Doppelklick oder Rechtsklick → „Bearbeiten“ wieder öffnen.
- Körper aus Skizzen: Rechtsklick auf die Skizze → „Extrudieren …“ / „Drehen …“ oder „Erzeugen → Skizze extrudieren … / Skizze drehen …“. Der Dialog listet die geschlossenen Profile mit Fläche und Löchern und erklärt, warum andere Kurvenzüge nicht verwendbar sind. Das Ergebnis ist ein normaler Körper, der sich mit den Booleschen Operationen weiterverarbeiten lässt.

**Was (noch) nicht geht**
- Ellipsen und Splines werden nicht zu Körpern (der Kernel kennt die Flächen dafür noch nicht); Profile müssen einfache geschlossene Ketten aus Linien und Bögen oder Kreise sein (keine T-Verzweigungen, keine Kreuzungen, kein Zerschneiden an Schnittpunkten).
- Drehen nur für Profile ohne Löcher, die mit genau einer Linie auf der Drehachse liegen und ganz auf einer Seite davon; immer 360°.
- Skizzen nur auf den Grundebenen und auf ebenen Körperflächen.
- Körper aus Skizzen sind Momentaufnahmen: Eine spätere Änderung der Skizze ändert den Körper nicht. Kopierte Körperkanten sind ebenso nicht mit dem Körper verknüpft.
- Kein Rückgängig, kein Speichern und Laden.
- Maße nur als Zahl, keine Gleichungen oder Parameter.
- Mehrdeutige Lagen (z. B. ein Winkel um 180° gedreht) entscheidet der nächstgelegene Zustand; es gibt keine Auswahl der Lösung.
- Der Solver rechnet je zusammenhängender Gruppe mit dichten Matrizen. Unveränderte Gruppen werden nicht neu gerechnet (auch beim Ziehen nicht); innerhalb einer Gruppe wird flüssiges Ziehen ab etwa 150–300 Variablen zäh (300 Variablen: etwa 0,15 s je Zugschritt im Release-Build). Widersprüchliche Gruppen sind langsamer: Beim Ziehen sucht der Solver dort nicht nach der Ursache, das holt er am Ende des Ziehens nach; hat eine Gruppe mehr als 48 Abhängigkeiten, meldet er statt der genauen Ursache „nicht lösbar“ mit den verletzten Abhängigkeiten. Ein Muster mit 100 Kopien eines Rechtecks ist eine Gruppe mit über 800 Variablen (Anlegen etwa 1,7 s, Ziehen etwa 1,5 s je Schritt).
- Fertige Skizzen liegen in der 3D-Ansicht immer über den Körpern (keine Verdeckung) und lassen sich dort nicht anklicken.

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Nach Teilprojekt 4 (Skizzen)“.

Lizenz: GPL-3.0-only (siehe `LICENSE`).

## Bauen

    cargo run -p planora-app --release

Unter Linux werden zusätzlich die System-Bibliotheken aus `.github/workflows/ci.yml` benötigt.

## Entwicklung

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo deny check

## Drittanbieter
- Schrift: Roboto 2.138, Apache-2.0 (siehe `crates/planora-app/assets/fonts/`).
