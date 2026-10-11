# PlanoraCAD

Ein quelloffenes, parametrisches 3D-CAD in reinem Rust für Windows und Linux.

**Status:** frühe Entwicklung (Fundament, Kernel-Basis, Boolesche Operationen, Skizzen mit Constraint-Solver, Feature-Baum, Speichern mit Projekten und Versionen).

Erste Version: Quader, Zylinder, Kugel, Kegel, Extrusion und Rotation von Skizzen, 3D-Ansicht mit Auswahl, Körper verschieben (Feld „Position“) sowie Boolesche Operationen (Vereinigen, Abziehen, Schneiden) für Körper mit Flächen aus Ebenen, Zylindern, Kegeln, Kugeln und Tori (Stufe A).

Bekannte Grenzen: Schnitte, die Raumkurven ergeben (z. B. zwei Zylinder mit gekreuzten Achsen), werden noch nicht unterstützt; das Vereinigen von Körpern, die sich nur an einer Kante berühren, manche genau tangentialen Lagen und Kanten oder Ecken, die genau auf einer Fläche des anderen Körpers aufliegen, liefern eine Fehlermeldung statt eines Ergebnisses (bei Berührung nur an einer Ecke entsteht ein Körper aus zwei Teilen mit gemeinsamer Ecke); es gibt noch kein Rückgängig. Details: `docs/superpowers/offene-punkte.md`.

## Skizzen

Über „Erzeugen → Skizze auf XY-/XZ-/YZ-Ebene“ oder „Skizze auf gewählter Fläche“ (ebene Fläche eines Körpers) entsteht eine Skizze. Im Skizzen-Modus schaut die Kamera senkrecht auf die Ebene; ein Fenster „Skizze“ enthält Werkzeuge, Abhängigkeiten und deren Liste.

**Was geht**
- Zeichnen: Punkt, Linie, Rechteck, Kreis, Bogen, Ellipse, Spline; Konstruktionsgeometrie über „Konstruktion umschalten“; Spiegeln und lineares Muster.
- Abhängigkeiten: deckungsgleich, waagerecht, senkrecht, parallel, rechtwinklig, tangential, gleich, fest, Mittelpunkt, symmetrisch, Punkt auf Kurve; Maße für Abstand, Abstand Punkt–Linie, Radius, Durchmesser und Winkel (Eingabe als Zahl).
- Der eigene Solver (keine neue Bibliothek) zeigt die Freiheitsgrade an, färbt bestimmte Geometrie anders ein und meldet überzählige und widersprüchliche Abhängigkeiten. Der Solver arbeitet je zusammenhängender Gruppe: Eine Gruppe mit Widerspruch (oder die sich nicht lösen lässt) bleibt in ihrem letzten Zustand, die übrigen Gruppen werden trotzdem gelöst. Lösungen, bei denen Geometrie entartet (Radius null oder negativ, Linie der Länge null, Bogenanfang auf der Mitte), gelten nicht als Lösung; die Gruppe bleibt dann ebenfalls stehen, und die Ursache wird genannt. Eine neue Abhängigkeit, die eine gelöste Skizze kaputt machen würde, wird abgelehnt; in einer schon fehlerhaften Skizze wird sie mit Warnung behalten. Ziehen startet vom letzten Zustand, mehrdeutige Lagen klappen in der Regel nicht um.
- Körperkanten einmalig als feste Konstruktionsgeometrie in die Skizze kopieren (Geraden; Kreise und Bögen in parallelen Ebenen; ganze Kreise und Ellipsen auch schräg, dann als Ellipse).
- „Skizze beenden“ legt die Skizze als Eintrag im Feature-Baum ab; sie bleibt als dünne Linien in der 3D-Ansicht sichtbar und lässt sich per Doppelklick oder Rechtsklick → „Bearbeiten“ wieder öffnen.
- Körper aus Skizzen: Rechtsklick auf die Skizze → „Extrudieren …“ / „Drehen …“ oder „Erzeugen → Skizze extrudieren … / Skizze drehen …“. Der Dialog listet die geschlossenen Profile mit Fläche und Löchern und erklärt, warum andere Kurvenzüge nicht verwendbar sind. Das Ergebnis ist ein normaler Körper, der sich mit den Booleschen Operationen weiterverarbeiten lässt und der Skizze folgt (siehe „Feature-Baum“).

**Was (noch) nicht geht**
- Ellipsen und Splines werden nicht zu Körpern (der Kernel kennt die Flächen dafür noch nicht); Profile müssen einfache geschlossene Ketten aus Linien und Bögen oder Kreise sein (keine T-Verzweigungen, keine Kreuzungen, kein Zerschneiden an Schnittpunkten).
- Drehen nur für Profile ohne Löcher, die mit genau einer Linie auf der Drehachse liegen und ganz auf einer Seite davon; immer 360°.
- Skizzen nur auf den Grundebenen und auf ebenen Körperflächen.
- Kopierte Körperkanten folgen der Kante nur, solange sie ihre Art behält (eine Gerade bleibt eine Gerade).
- Kein Rückgängig.
- Maße nur als Zahl, keine Gleichungen oder Parameter.
- Mehrdeutige Lagen (z. B. ein Winkel um 180° gedreht) entscheidet der nächstgelegene Zustand; es gibt keine Auswahl der Lösung.
- Der Solver rechnet je zusammenhängender Gruppe mit dichten Matrizen. Unveränderte Gruppen werden nicht neu gerechnet (auch beim Ziehen nicht); innerhalb einer Gruppe wird flüssiges Ziehen ab etwa 150–300 Variablen zäh (300 Variablen: etwa 0,15 s je Zugschritt im Release-Build). Widersprüchliche Gruppen sind langsamer: Beim Ziehen sucht der Solver dort nicht nach der Ursache, das holt er am Ende des Ziehens nach; hat eine Gruppe mehr als 48 Abhängigkeiten, meldet er statt der genauen Ursache „nicht lösbar“ mit den verletzten Abhängigkeiten. Ein Muster mit 100 Kopien eines Rechtecks ist eine Gruppe mit über 800 Variablen (Anlegen etwa 1,7 s, Ziehen etwa 1,5 s je Schritt).
- Fertige Skizzen liegen in der 3D-Ansicht immer über den Körpern (keine Verdeckung) und lassen sich dort nicht anklicken.

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Nach Teilprojekt 4 (Skizzen)“.

## Feature-Baum

Alles, was du baust, steht als Liste von Schritten im Feature-Baum: Quader, Skizze, Extrusion, Drehung, Boolesche Operation, Verschiebung. Die Reihenfolge ist die Reihenfolge der Entstehung.

**Was geht**
- Zeitleiste: Jeder Schritt baut auf den früheren auf. Ein Schritt kann mit Rechtsklick umbenannt, bearbeitet oder gelöscht werden.
- Neuberechnung: Änderst du ein Maß (z. B. in einer Skizze oder die Höhe einer Extrusion), rechnen sich alle Schritte danach neu. Nur das, was sich wirklich ändert, wird neu gebaut.
- Feste Verweise: Eine Skizze auf einer Körperfläche oder eine Kantenkopie merkt sich die Fläche bzw. Kante und findet sie nach einer Änderung wieder.
- Unterdrücken: Ein Schritt lässt sich abschalten. Schritte, die von ihm abhängen, werden dann übersprungen.
- Zurückrollen: Über „Zurückrollen bis hier“ siehst du den Stand früherer Schritte; „Zum Ende“ holt alles zurück.
- Löschen mit Folgen: Beim Löschen zeigt das Programm, welche weiteren Schritte davon betroffen sind, und fragt nach.
- Fehler stehen an dem Schritt, der sie auslöst. Alles davor bleibt benutzbar; Schritte, die davon abhängen, werden übersprungen.

**Was (noch) nicht geht**
- Kein Rückgängig.
- Keine benannten Maße oder Gleichungen.
- Keine Muster- oder Spiegel-Schritte.
- Der Zurückroll-Marker lässt sich nur über das Menü setzen, nicht mit der Maus ziehen.
- Die Ebene einer Skizze lässt sich nachträglich nicht ändern.
- Ein Körper lässt sich nicht nachträglich vor eine Skizze setzen, die ihn braucht.
- Verschiebst du einen Körper im Eigenschaftenfenster, kommt ein neuer Schritt ans Ende; Skizzen und Extrusionen davor ziehen nicht mit. Änderst du dagegen die Maße des Quaders selbst, folgen sie.
- Die Oberfläche des Feature-Baums wurde zuletzt von Hand durchgespielt (mit `tools/gui-drive.ps1`): Quader anlegen und ändern, Skizze auf einer Fläche, Extrusion, Vereinigen, Unterdrücken, Zurückrollen, Löschen mit Rückfrage und Position ziehen liefen ohne Absturz. Rotation und Fehlerfälle wurden nur durch automatische Tests geprüft.

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Nach Teilprojekt 5 (Feature-Baum)“.

## Speichern, Projekte und Versionen

Deine Arbeit bleibt jetzt erhalten. Gespeichert wird die Liste der Schritte aus dem Feature-Baum; beim Öffnen rechnet das Programm daraus alles neu.

**Wo die Dateien liegen**
- Alles liegt in einem normalen Ordner auf deiner Festplatte, vorgegeben ist `Dokumente\PlanoraCAD`. Im Fenster „Projekte“ kannst du einen anderen Ordner wählen („Wurzel wählen“). Das Programm merkt sich die Wahl.
- Ein **Projekt** ist ein Ordner direkt darin. Darin kannst du beliebig viele **Ordner** anlegen und verschachteln. Ein **Dokument** ist ein Ordner mit der Endung `.pcad`; darin liegen die Versionen als einzelne Textdateien. Du kannst den ganzen Projektordner also einfach kopieren oder sichern.
- Gelöschte Projekte, Ordner und Dokumente landen im Ordner `.papierkorb` unter dem Projektordner und werden vom Programm nie endgültig gelöscht. Gelöschte Versionen und Zweige im Fenster „Versionen“ sind dagegen nach der Rückfrage endgültig weg.

**Speichern und Versionen**
- Mit „Datei → Speichern“ (Strg+S) legst du eine **neue Version** an; dabei kannst du einen Kommentar eintragen. Frühere Versionen bleiben unverändert erhalten.
- Ein neues Dokument heißt „Unbenannt 1“. Beim ersten Speichern wählst du Projekt, Ordner und Namen. „Speichern unter …“ legt ein eigenes neues Dokument an.
- Ein „●“ am Tab zeigt, dass es ungespeicherte Änderungen gibt. Beim Schließen eines Tabs oder des Programms fragt PlanoraCAD nach (Speichern, Verwerfen, Abbrechen). Eine gerade offene Skizze wird beim Speichern mitgenommen.
- Das Fenster „Versionen“ (Menü „Ansicht“) listet alle Versionen des Dokuments mit Nummer, Zweig, Zeit und Kommentar. Ein Doppelklick öffnet eine alte Version **nur zum Ansehen** in einem eigenen Tab („Name · v1“); dort lässt sich nichts ändern.
- Per Rechtsklick auf eine Version kannst du sie **wiederherstellen** (sie wird als neueste Version noch einmal gespeichert, nichts geht verloren), von ihr aus einen **Zweig abzweigen** (eine Variante mit eigenem Namen), den Kommentar ändern oder eine Version bzw. einen Zweig löschen, solange keine andere Version darauf aufbaut.
- Speicherst du ein Dokument, das auf einer älteren Version beruht, fragt das Programm nach dem Namen eines neuen Zweigs, damit nichts überschrieben wird.
- Zusammenführen von Zweigen gibt es noch nicht.

**Mehrere Dokumente und Absicherung**
- Mehrere Dokumente können gleichzeitig in Tabs offen sein. Das Fenster „Projekte“ (Menü „Ansicht“ oder Strg+O) zeigt den Baum aus Projekten, Ordnern und Dokumenten; Doppelklick öffnet, Rechtsklick bietet Anlegen, Umbenennen, Verschieben und Löschen.
- Ein Dokument kann nur von einem Programm zur gleichen Zeit geändert werden. Ist es schon gesperrt, fragt PlanoraCAD, ob du die Sperre übernehmen willst.
- Alle zwei Minuten sichert das Programm geänderte, schon gespeicherte Dokumente automatisch (das ist keine neue Version). Nach einem Absturz bietet es beim nächsten Öffnen an, diesen Stand zurückzuholen oder zu verwerfen.
- Beschädigte oder zu neue Dateien werden mit einer Meldung abgelehnt; das geöffnete Dokument bleibt dabei unverändert.

**Was (noch) nicht geht**
- Kein Zusammenführen von Zweigen, kein Teilen über das Internet, keine Vorschaubilder, die Kameraansicht wird nicht gespeichert.
- Unbenannte Dokumente (noch nie gespeichert) haben keine Autosicherung.
- Umbenennen, Verschieben und Löschen im Projektbaum geht nur, wenn die betroffenen Dokumente geschlossen sind. Verschoben wird über ein Auswahlfenster, nicht durch Ziehen.

**Handprüfung:** Am 11.10.2026 von Hand im echten Fenster geprüft (mit `tools/gui-drive.ps1`): Projekt anlegen, Quader anlegen, Strg+S und „Speichern unter“, zweite Version mit Kommentar, Versionen-Panel, alte Version öffnen (schreibgeschützt, Menüs ausgegraut), Wiederherstellen, Fenster-× mit ungespeicherten Änderungen (Rückfrage, das Programm bleibt offen), „Speichern“ in der Rückfrage beendet das Programm und gibt die Sperre frei, Neustart und Öffnen aus dem Projekt-Panel (der Stand ist vollständig da), × ohne Änderungen schließt sofort. Nicht von Hand geprüft (nur durch automatische Tests ohne Fenster): Abzweigen mit Zweignamen-Fenster, Zweig oder Version löschen, Kommentar ändern, Ordner anlegen, umbenennen, verschieben und löschen im Projekt-Panel, Autosicherungs-Angebot nach einem Absturz, Sperre übernehmen, Schließen bei minimiertem Fenster, mehrere Tabs mit mehreren Dokumenten.

Einzelheiten: `docs/superpowers/offene-punkte.md`, Abschnitt „Teilprojekt 6 – Speichern und Projekte“.

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
