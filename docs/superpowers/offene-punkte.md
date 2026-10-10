# Offene Punkte nach Teilprojekt 1–5

Stand: 2026-10-10 (Teilprojekt 5 siehe eigener Abschnitt weiter unten). Teilprojekt 1–4: 2026-10-09. Ergebnis der Abschlussprüfungen; bewusst nicht in Teilprojekt 1–4 behoben.

## Entscheidung des Projekteigners nötig
- **Markenrecherche** für „PlanoraCAD“ (DPMA, EUIPO, WIPO, Klasse 9) vor einer Veröffentlichung.
- **Linux:** App-Start dort noch nicht geprüft (CI baut und testet nur).

## Beobachten
- **`ttf-parser 0.25.1`** (RUSTSEC-2026-0192, „unmaintained“, keine Sicherheitslücke) wird nur auf dem Linux/Wayland-Pfad gezogen (winit 0.30 → sctk-adwaita 0.10 → ab_glyph) und ist in `deny.toml` begründet ignoriert. Erneut prüfen, sobald winit 0.31 stabil und übernommen ist (nutzt sctk-adwaita ≥ 0.12, das `skrifa` verwenden kann); dann den Eintrag entfernen.

## Bekannte Grenzen
- Roboto 2.138 enthält die Zeichen U+23F4 ⏴ / U+23F5 ⏵ (von `egui_tiles` für die Überlauf-Pfeile der Tab-Leiste und von `egui` für Untermenü-Pfeile genutzt) und U+2192 → nicht. Heute ohne Folgen (keine Untermenüs, ein Tab je Container), würde aber als leere Kästchen erscheinen. Lösung bei Bedarf: Pfeile als Formen zeichnen oder eine Ersatzschrift ergänzen.

## Aus der Liste „Vor Teilprojekt 3“ noch offen
Erledigt mit Teilprojekt 3: beliebig berandete gekrümmte Flächen (beschnittene Flächen samt Vernetzung); exakte Prädikate (`orient2d`) gibt es bisher nur für die Windungszahl (Punkt-in-Fläche). Weiterhin offen:
- Exakte Prädikate auch im ebenen Graphen: Die Sortierung der Kanten an einer Ecke in `boolean/split.rs` nutzt `atan2` mit festen Gleichstands-Schwellen (1e-9), nicht `orient2d`.
- Arena-getaggte IDs (IDs verschiedener Körper sind heute austauschbar).
- Validator: Kantenendpunkte gegen Eckpositionen prüfen (`curve.point(t0/t1)`).
- Schwellen aus `Tolerance` ableiten (es gibt noch feste Werte wie 1e-12 und 1e-9 im Kernel).
- **Benannte Toleranzen:** Feste Epsilon-Werte ohne Bezug zu `Tolerance` stehen noch in `boolean/split.rs`, `boolean/roots.rs`, `boolean/uv.rs` (Polerkennung 1e-7, Polsprung 1e-9), `boolean/imprint.rs`, `boolean/seg.rs`, `boolean/surfsurf.rs`, `geometry.rs` (`pole_vs`/Projektion) und `planora-mesh/src/trimmed.rs` (Beschnitt-Erkennung 1e-6 relativ, Verschweißen 1e-12, Unterlauf). Ziel: benannte Konstanten bzw. Ableitung aus `Tolerance`.
- **Validator:** prüft je Schale Euler-Charakteristik und Zugehörigkeit der Flächen, aber noch nicht Spec §7 „je Fläche nicht überlappende, korrekt orientierte Schleifen“ (äußere Schleife und Löcher im Parameterraum). Geplant als Folgearbeit.

## Nach Teilprojekt 3 (Boolesche Operationen)
Stand: 2026-10-09. Bewusst offen gelassen; Fehlerfälle liefern einen benannten Fehler, nie ein falsches Ergebnis.

**Was nicht geht (benannter Fehler statt Ergebnis)**
- Nur Stufe A: Schnitte von Ebene, Zylinder, Kegel, Kugel und Torus, soweit sie ebene Kurven oder Kreise ergeben. Andere Paare (Raumkurven, z. B. zwei Zylinder mit gekreuzten Achsen, schräge Kegel- oder Torus-Ebenenschnitte) liefern „Der Schnitt von … mit … wird noch nicht unterstützt“.
- Berührung nur an einer Kante, ohne gemeinsame Fläche (zwei Quader, Kante an Kante): Vereinigen liefert „Die Körper berühren sich nur an einer Kante oder tangential …“ (früher „Kante N wird 4-mal verwendet“), Abziehen liefert A unverändert, Schneiden „Das Ergebnis ist leer“ (festgeschrieben in `boxes_touching_only_at_an_edge_do_not_panic`). Als Berührung gemeldet wird eine mehrfach benutzte Kante nur, wenn kein Flächenstück im Inneren des anderen Körpers liegt; sonst bleibt es „Das Ergebnis ist ungültig“, damit echte Fehler nicht verdeckt werden.
- Tangentialkontakt mit Polen auf Kanten: Eine Kugel (r = 5), geschnitten mit einem gleich großen Quader genau durch ihre Mitte, berührt die Schnittfläche an deren Randkanten, und die Pole liegen auf Quaderkanten. Das ergibt heute den Berührungsfehler (Test `half_sphere_by_a_box` erlaubt auch ein korrektes Ergebnis); mit einem etwas größeren Quader klappt derselbe Schnitt (`half_sphere_by_a_larger_box`). Bei CAD-typischen „genau passenden“ Maßen kann das häufiger vorkommen. Nicht jede Tangentiallage scheitert: ein Zylinder, der eine Quaderseite außen berührt, liefert beim Vereinigen und Abziehen ein gültiges Ergebnis.
- Sehr dünne Bereiche: Findet die Einordnung auf ihrem 24×24-Raster keinen Innenpunkt eines Flächenstücks, endet die Operation mit „Die Lage eines Flächenstücks … ließ sich nicht sicher bestimmen“.
- Ecke genau auf einer Fläche des anderen Körpers (z. B. ein um 45° gedrehter Quader, dessen Kante bzw. Ecke genau auf einer Quaderseite aufliegt): liefert heute den Berührungsfehler („Die Körper berühren sich so, dass die Toleranz keine sichere Entscheidung zulässt“).

**Was geht, aber unschön ist**
- Gedrehte Nähte koaxialer Zylinder: Geschlossene Kanten werden beim Zusammennähen über Startpunkt und Mittelpunkt zugeordnet. Mit gegeneinander verdrehten Nähten beobachtet funktionsfähig; gezielt geprüft nur für Nähte, die um 0,7 rad verdreht sind (Test `coaxial_cylinders_with_rotated_seams`: Vereinigen, Abziehen, Schneiden, auch bei gleichem Radius).
- An einer Kegelspitze enthält das Netz flächenlose Dreiecke (Rasterpunkte einer Mantellinie liegen mit der Spitze auf einer Geraden). Sie halten das Netz geschlossen und ändern das Volumen nicht.
- Berühren sich zwei Körper nur an einer Ecke, besteht die Vereinigung aus zwei Schalen mit einer gemeinsamen Ecke (nicht mannigfaltig; Volumen stimmt; bewusst akzeptiert).
- Bündige Flächen werden nicht automatisch zusammengefasst (zusätzliche Kanten und Flächen nach dem Vereinigen).

**Geschwindigkeit**
- Zusammennähen und T-Knoten-Auftrennung arbeiten quadratisch in der Zahl der Kanten.
- Punkt-in-Fläche baut die Randpolygone bei jedem Aufruf neu auf (kein Zwischenspeicher).
- Auswahl (Pick) läuft über alle Dreiecke; bei großen Netzen nach vielen Operationen spürbar langsamer.

**Oberfläche**
- Beim Ziehen am Feld „Position“ wird der Körper in jedem Bild neu vernetzt (später das Netz nur verschieben).
- „Position“ zeigt die untere Ecke der Hülle des Netzes, nicht einen Bezugspunkt des Körpers.
- Kein Rückgängig; ein Ergebnis ersetzt beide Ausgangskörper.
- Der Dialog „Boolesche Operation“ merkt sich die Körper als Listenplatz; wird ein Körper gelöscht, während der Dialog offen ist, zeigt er auf einen anderen.

## Erledigt in Teilprojekt 4
Die Liste „Vor Teilprojekt 4“ ist abgearbeitet:
- `ProfileLoop::polygon` gibt `Result` zurück statt bei < 3 Punkten zu paniken.
- `ProfileLoop::sample`/`signed_area` sind abgesichert (endliche Werte, begrenzte Schrittzahl).
- Sich schneidende, außerhalb liegende und (neu) in anderen Löchern liegende Löcher werden abgewiesen (`KernelError`, u. a. `NestedHoles`).
- `extrude`/`revolve` prüfen `Frame`-Eingaben (endlich, orthonormal innerhalb der Toleranz).
- Die Oberfläche liest Flächen- und Kantenzahl über die Kernel-Fassade (`Kernel::counts`) statt über `solid.faces[..]`.

## Nach Teilprojekt 4 (Skizzen)
Stand: 2026-10-10 (nach der Abschlusswelle). Bewusst offen gelassen; Fehlerfälle liefern eine deutsche Meldung. Der Solver arbeitet je zusammenhängender Gruppe: Eine nicht lösbare oder widersprüchliche Gruppe bleibt in ihrem letzten Zustand, andere Gruppen werden trotzdem gelöst. Entartete Lösungen (Radius ≤ 0, Linie der Länge null, Bogenanfang/-ende auf der Mitte, Ellipsen-Halbachse null) gelten nicht als Lösung. Eine neue Abhängigkeit wird nur zurückgenommen, wenn die Skizze vorher gelöst war oder sie selbst als Ursache gemeldet wird; in einer schon fehlerhaften Skizze bleibt sie mit Warnung.

**Funktionsumfang (Grenzen)**
- Ellipsen und Splines werden nicht zu Körpern („Dieses Profil enthält … und kann noch nicht zu einem Körper werden“); der Kernel kennt elliptische Zylinder und NURBS-Flächen noch nicht.
- Profile müssen einfache geschlossene Ketten aus Linien und Bögen oder Kreise sein: keine T-Verzweigungen, keine Kreuzungen, kein Zerschneiden von Kurven an Schnittpunkten. Schleifen ohne Fläche oder ohne Schluss gelten als entartet.
- Drehen nur für Profile ohne Löcher, mit genau einer Linie auf der Drehachse und ganz auf einer Seite der Achse; immer 360°. Extrudieren nur in Richtung der Skizzennormalen (kein Gegen- oder Symmetrisch-Extrudieren).
- Skizzen nur auf den Grundebenen und auf ebenen Körperflächen; die Ebene einer Skizze lässt sich nachträglich nicht verschieben.
- Körperkanten werden nur einmalig kopiert (Geraden; Kreise und Bögen in parallelen Ebenen; ganze Kreise und Ellipsen auch schräg, dann als Ellipse). Schräge Bögen und Ellipsenbögen werden abgelehnt. (Seit Teilprojekt 5 folgen die Kopien der Kante, solange sie ihre Art behält.)
- (Erledigt in Teilprojekt 5:) Körper aus Skizzen rechnen sich bei Änderung der Skizze neu; siehe Abschnitt „Nach Teilprojekt 5“.
- Kein Rückgängig, kein Speichern/Laden.
- Mehrdeutige Lagen werden vom nächstgelegenen Zustand entschieden: Der Winkel-Rest ist auch bei θ + π null (gespiegelte Lösung), ebenso bei `EllipseAngle`; es gibt keine Auswahl der Lösung.
- Maße nur als Zahl; keine Gleichungen, Parameter oder Ausdrücke.
- Konstruktionsgeometrie nur über den Umschalter „Konstruktion umschalten“ (kein eigenes Werkzeug).
- Ein Punkt auf einem Spline, der über dessen Ende hinausgezogen wird, bleibt dort stehen, wo er zuletzt gültig war (er rutscht nicht ans Ende; keine Ungleichungs-Abhängigkeiten). Ebenso bleibt ein Punkt stehen, wenn der Zug Geometrie entarten ließe (z. B. Kreismitte durch die Tangente hindurch).

**Solver und Algebra**
- Dichte Matrizen je zusammenhängender Gruppe. Ergebnisse je Gruppe werden mit den Variablenwerten gemerkt; unveränderte Gruppen rechnen nicht neu (Zähler `SolveReport::group_solves`). Innerhalb einer Gruppe wird Ziehen ab etwa 150–300 Variablen zäh (Release, geschlossene Kette mit Abständen, 300 Variablen: Lösen etwa 30 ms, Zugschritt etwa 150 ms). Ein Muster mit 100 Rechteck-Kopien (808 Variablen, eine Gruppe): Anlegen etwa 1,7 s (das Muster löst einmal selbst, um überzählige Abhängigkeiten zu entfernen), Zugschritt etwa 1,5 s.
- Widerspruchsdiagnose: Sie löst die Gruppe je nutzererzeugter Abhängigkeit einmal neu. Beim Ziehen entfällt sie für die gezogene Gruppe und läuft beim Loslassen (volles Lösen in `end_drag`); andere Gruppen kommen aus dem Speicher. Bei mehr als 48 Abhängigkeiten in einer Gruppe (`MAX_DIAGNOSE_RUNS`) entfällt die Einzelsuche: Status „nicht lösbar“ mit den Abhängigkeiten, deren Residuum zu groß ist. Zugschritte in einer schon widersprüchlichen Gruppe kosten trotzdem drei gescheiterte Läufe (200 Variablen etwa 70 ms, 300 etwa 220 ms).
- Der Zustand gilt je Gruppe: Ziehen kann „widersprüchlich“ melden, während eine andere Gruppe sich bewegt hat; die Oberfläche muss den Status lesen.
- Abhängigkeiten mit Jacobi-Zeile null werden als überzählig gemeldet; interne Abhängigkeiten (z. B. Bogenradius) nie.
- Zusatzvariablen entfernter Abhängigkeiten (Spline- und Ellipsen-Parameter) werden nicht aufgeräumt.
- Cholesky liest nur das untere Dreieck (setzt Symmetrie voraus); die Nullraum-Bestimmung orthogonalisiert nur einmal und kann bei frühem Abbruch weniger als n − Rang Vektoren liefern.
- `Dual::variable` panict bei Index ≥ n (interne API); `sqrt` klemmt NaN auf 0 und liefert für negative Radikanden einen sehr großen Schein-Gradienten; `div` hat keinen Null-Schutz (die Abhängigkeiten teilen nur durch abgesicherte Längen).
- Spline-Parameter: Der Spielraum hängt von der Länge der Endsehnen ab (sehr kurze Endsehnen < 1e-5 mm sind gedeckelt); `tol.linear == 0` ergäbe NaN; der Klemm-Zweig ist ungetestet; `spline_parameter` kopiert die Punktliste je Aufruf.
- Kleinere Ungereimtheiten: `measure` kann genau −π liefern; `Symmetric(a, a)` meldet „Falscher Objekttyp“ statt „Entartet“; Entfernen interner Abhängigkeiten meldet „Das Objekt gibt es nicht“.
- Ziehen: Ein Punkt, der in einem Bild zweimal gezogen wird, ist undokumentiert; schlägt der Feinschliff fehl und der Neustart konvergiert, wird das Ziehen still ignoriert; `point_unconstrained` und die Gruppenbildung behandeln Abhängigkeiten ohne lokale Variablen unterschiedlich.
- Fehlende Tests: Umklappen über eine Zweiggrenze mit freiem Freiheitsgrad (tangential/Punkt-Linie), Ziehen eines freien Freiheitsgrads in einer bestimmten Gruppe, negativer Zweig von Punkt-Linien-Abstand und innerer Kreistangente.

**Profile und Körper**
- Endpunkte werden gierig (nicht transitiv) zu Knoten zusammengefasst; zwei Schleifen, die sich in einem zusammengefassten Knoten treffen, gelten als „nicht eindeutig“.
- Die Schnittprüfung der Schleifen ist quadratisch (250 konzentrische Kreise brauchen Sekunden). Kernel und Skizze benutzen dieselbe Prüfung (`planora_kernel::SampledLoop`): Schleifen, die sich näher kommen als `tol.linear` plus die Sehnenabweichung der Abtastung (64 Schritte je Vollkreis, etwa 0,12 % des Radius), gelten als sich berührend. Sehr knappe echte Abstände zwischen Bögen werden damit ebenfalls abgelehnt; sehr große Bögen in Profilen mit kleinen Einzelheiten können so zu früh als „berührend“ gelten.
- Die Reihenfolge der abgelehnten Schleifen ist nicht global sortiert; die Meldung zum Öffnungswinkel ist nahe 2π ungenau.
- Drehen: Die Achsprüfung kommt vor der Seitenprüfung (eine Achse quer durchs Profil meldet „keine Kante auf der Achse“); ein zweites kollineares Achsstück lehnt erst der Kernel ab. Kein Test für einen gespiegelten Halbkreis (Drehsinn −1).
- Kernel: Die Frame-Prüfung nutzt `tol.linear` als Orthonormalitäts-Schwelle (künftig benannte Konstante); in `check_loop_input` und `extrude` stehen noch feste Werte (1e-9, 1e-12).

**Hilfen (Spiegeln, Muster, Kanten kopieren)**
- Liegt ein Punkt auf der Spiegelachse, wird er nicht an die Achse gebunden; Ziehen kann die Symmetrie dann brechen. Eine Kurve auf sich selbst zu spiegeln erzeugt Doppelte.
- Die Projektion eines Quaders erzeugt deckungsgleiche doppelte Linien; die Bereichsprüfung des Linienparameters mischt Einheiten.

**Oberfläche**
- Fertige Skizzen werden in der 3D-Ansicht als Linien über alles gezeichnet (keine Verdeckung durch Körper) und lassen sich dort nicht anklicken.
- Die Dialoge „Skizze extrudieren/drehen“ und „Boolesche Operation“ merken sich Skizze bzw. Körper als Listenplatz; wird währenddessen gelöscht, zeigen sie auf einen anderen Eintrag. Eine Drehachse, die nicht (mehr) zur gewählten Skizze gehört, wird mit Meldung abgelehnt.
- Fertige Skizzen werden in der 3D-Ansicht jedes Bild neu in Linien zerlegt; Linienstücke mit einem Endpunkt hinter der Kamera fallen weg (kein Abschneiden an der Nahebene).
- Körper entstehen nur außerhalb des Skizzen-Modus (die offene Skizze wird vorher beendet).
- Hinweise erscheinen in der Statuszeile und im Skizzenfenster ebenfalls rot (keine Unterscheidung Hinweis/Fehler).
- Esc schließt das Zahlenfeld für Maße nicht (es schließt bei neuer Auswahl oder mit Eingabe).
- Roboto enthält `∥` und `⟂` nicht; angezeigt werden `||` und „Lot“. Beschriftungen an derselben Stelle können sich bei dichten Skizzen überlappen.
- Ein Klick auf den Navigationswürfel im Skizzen-Modus kippt die Kamera (auch Ziehen am Würfel ist im Skizzen-Modus nicht gesperrt); danach ist die Sicht nicht mehr senkrecht, bis die Skizze neu geöffnet wird. Mit großer Skizze erscheint die z-Achse des Gitters in der Draufsicht als schräge Linie.
- Die Kurvenfarben der Skizze sind fest im Code hinterlegt (nicht Teil des Designs/Themes).
- Ein neuer, nicht konvergierender Teil neben einem schon fehlerhaften Teil wird beim Prüfen einer Änderung nur erkannt, wenn er dabei neue verletzte Abhängigkeiten nennt (der Vergleich nutzt den letzten Lösungsbericht der Sitzung); in einer fehlerhaften Skizze werden keine automatischen Waagerecht-/Senkrecht-Abhängigkeiten angelegt, und die Ziehen-Warnung erscheint immer.
- `status()` löst eine Kopie der Skizze, wenn noch kein Lösungsbericht vorliegt.
- Von Hand nicht geprüft: Eingabe/Esc/Entf-Tasten (die Automatisierung konnte sie nicht senden), Verschieben mit der mittleren Maustaste, Zoomen, Spiegeln/Muster, Kanten kopieren, helles Design.
- Fehlende Tests: Ausmaß-Berechnung nur für Kreise (nicht Bögen/Ellipsen), Werkzeugfolge ohne Abschluss, Aufheben der Auswahl im Browser, Bearbeiten einer bestehenden Abhängigkeit über das Zahlenfeld; erneutes Anklicken derselben Fläche löscht die Skizzenhervorhebung nicht.

## Nach Teilprojekt 5 (Feature-Baum)
Stand: 2026-10-10. Skizzen, Extrusionen, Drehungen, Boolesche Operationen und Verschiebungen sind jetzt Features in einer Zeitleiste; sie rechnen sich neu, wenn ein früheres Feature geändert wird. Fehler werden an dem Feature gemeldet, das sie verursacht; die übrigen Features bleiben benutzbar.

**Funktionsumfang (Grenzen)**
- Kein Speichern und Laden, kein Rückgängig, keine benannten Parameter oder Gleichungen, keine Muster- oder Spiegel-Features.
- Der Rückrollmarker lässt sich nur per Menü setzen, nicht mit der Maus ziehen.
- Die Ebene einer Skizze lässt sich nachträglich nicht ändern. Ein Körper lässt sich nicht vor die Skizze „projizieren“, die ihn braucht.
- Die festen Beispielprofile sind entfernt; Profile kommen nur noch aus Skizzen.
- Körper aus Booleschen Operationen haben die Art `BodyKind::Combined` und keine eigenen Maße, die sich nachträglich bearbeiten ließen.

**Verweise (stabile Namen)**
- Das Profil einer Extrusion oder Drehung wird über die Nummer in der Profilliste gefunden. Ändert sich die Reihenfolge der Profile (z. B. durch eine geänderte Skizze), kann das Feature ein anderes Profil meinen.
- Vertauschen sich zwei gleichartige Flächen (gleiche Rolle, gleiche Herkunft), kann ein Verweis auf die falsche zeigen; der Hinweispunkt entscheidet nur bei mehreren Treffern.
- Eine Skizze auf einer Fläche hängt nur von dem Grundkörper ab, der die Fläche erzeugt hat. Wird ein später entstandenes Feature gelöscht (z. B. eine Vereinigung), erscheint es nicht in der Liste der Folgen.
- `Move` behält keine Körperidentität: Nach einer Verschiebung ist der Körper ein neuer, die Verweise laufen über die Marken der Flächen.
- Kantenkopien in Skizzen folgen der Kante nur, solange sie die gleiche Art behält (Gerade bleibt Gerade, Kreis bleibt Kreis); sonst meldet das Skizzen-Feature einen Fehler. Wird das Feature gelöscht, das die Kante erzeugt hat, verlieren die Kopien ihre Verknüpfung und bleiben als feste Linien in der Skizze.
- Ein Hinweispunkt kann bei sehr schmalen Flächen fehlen; dann bleibt der alte Hinweis stehen.

**Zeitleiste und Neuberechnung**
- Der Zwischenspeicher gilt nur im Speicher und geht beim Beenden verloren.
- Ein `Move`, das nach einer abhängigen Skizze oder Extrusion hinzukommt, verschiebt diese nicht mit (Zeitleiste: ein späteres Feature ändert frühere nicht). Wer will, dass die Skizze folgt, ändert die Maße im Quader-Feature selbst.
- Die Volumenprobe (2 % Abweichung) kann sehr kleine gekrümmte Körper zu Unrecht ablehnen.
- Das Ergebnis einer Booleschen Operation oder Verschiebung nimmt den Listenplatz seines ersten Eingangs ein; die Oberfläche darf nicht auf feste Plätze bauen.
- Kopien einer Skizze verlieren ihre Verknüpfung, wenn ihr Kanten-Feature gelöscht wird (siehe oben).
- Boolesche Operationen heißen „Boolesche Operation N“; die Art steht nicht im Namen.
- Der Zweig, der eine fehlgeschlagene Verschiebung zurücknimmt (`move_body`), ist nicht getestet; riesige Verschiebungen werden vorher abgelehnt (±1e9 mm).

**Oberfläche**
- Beim Ziehen am Feld „Position“ werden alle Körper in jedem Bild neu vernetzt (später Netze zwischenspeichern).
- Verschiebt man einen Körper im Eigenschaftenfenster, während eine Skizze offen ist, wird die Skizze beendet.
- Löscht man die offene Skizze, geht ihr Entwurf ohne Nachfrage verloren.
- „Löschen“ beendet die offene Skizze auch dann, wenn die Rückfrage abgebrochen wird.
- Die offene Skizze wird auch dann verworfen, wenn ihr Entwurf von dem gelöschten Feature gar nicht mehr abhängt.
- `extrude_sketch`/`revolve_sketch` und einige Menüaufrufe nutzen noch `close_sketch()` und überhören einen abgelehnten Entwurf; richtig wäre `try_close_sketch()`.
- Fehlt dem Skizzen-Feature ein Ergebnis, nutzt die Oberfläche die gespeicherte Definition.
- **Von Hand nicht geprüft:** Die Oberfläche des Feature-Baums wurde in Aufgaben 7–9 nie von Hand durchgespielt, weil die Bildschirmsteuerung unterbrochen wurde. Abgesichert ist sie nur durch automatische Tests (App ohne Fenster).

**Aktualisierung früherer Abschnitte**
- „Körper aus Skizzen sind Momentaufnahmen“ und „Keine lebende Verknüpfung“ gelten nicht mehr: Skizzen sind Features, Körper und Kantenkopien folgen Änderungen (Grenzen oben).
- „Kein Rückgängig“ und „Kein Speichern/Laden“ gelten weiter.

## Aufräumen / Qualität
- `rust-version = "1.88"` ist für die GUI-Crates ungenau (egui 0.36 braucht 1.95).
- `eframe` mit Standard-Features baut auch den `glow`-Renderer mit.
- CI baut `cargo-deny` bei jedem Lauf neu.
- Tests: Pan-/Orbit-Vorzeichen, Auswahl vor Hover, Vertex-Größen, `UnsupportedSegment`, Torus bei Uhrzeigersinn-Kette.
- Auswahl: CPU-Raycast läuft bei jeder Mausbewegung über alle Dreiecke (später Hüllkörper/Raumindex).
- Hover-Änderung baut die ganze Szene neu und lädt sie hoch (später nur Farben aktualisieren).
- Browser: lange Namen werden abgeschnitten; Klick im Browser setzt keine Flächenauswahl.
