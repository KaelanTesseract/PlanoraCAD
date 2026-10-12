# Offene Punkte nach Teilprojekt 1–7

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
- Kantenkopien in Skizzen folgen der Kante nur, solange sie die gleiche Art behält (Gerade bleibt Gerade, Kreis bleibt Kreis); sonst meldet das Skizzen-Feature einen Fehler. Wird das Feature gelöscht, von dem die Marken der Kante stammen (z. B. der Quader), zählt die Skizze als abhängig: Sie wird nach Rückfrage samt ihren Extrusionen mitgelöscht. Wird dagegen eine Boolesche Operation oder ein Verschieben gelöscht, dessen Ergebniskante kopiert wurde, bleibt die Skizze stehen und meldet danach „Kante nicht mehr vorhanden“ (`EdgeNotFound`), wenn es die Kante am ursprünglichen Körper so nicht gibt.
- Ein Hinweispunkt kann bei sehr schmalen Flächen fehlen; dann bleibt der alte Hinweis stehen.

**Zeitleiste und Neuberechnung**
- Der Zwischenspeicher gilt nur im Speicher und geht beim Beenden verloren.
- Ein `Move`, das nach einer abhängigen Skizze oder Extrusion hinzukommt, verschiebt diese nicht mit (Zeitleiste: ein späteres Feature ändert frühere nicht). Wer will, dass die Skizze folgt, ändert die Maße im Quader-Feature selbst.
- Die Volumenprobe (2 % Abweichung) kann sehr kleine gekrümmte Körper zu Unrecht ablehnen.
- Das Ergebnis einer Booleschen Operation oder Verschiebung nimmt den Listenplatz seines ersten Eingangs ein; die Oberfläche darf nicht auf feste Plätze bauen.
- Kantenkopien nach dem Löschen: siehe „Verweise“ oben (Herkunfts-Feature gelöscht → Skizze wird mitgelöscht; Boolesche Operation/Verschieben gelöscht → Skizze bleibt und kann `EdgeNotFound` melden).
- Die Neuberechnung invalidiert großzügig: Ändert sich ein Körper, rechnen alle Features neu, die sichtbare Körper lesen (Skizzen auf Flächen, Kantenkopien, Boolesche Operationen), auch wenn sie den geänderten Körper gar nicht brauchen.
- Umbenennen rechnet nicht neu; zwischengespeicherte Fehlermeldungen nennen deshalb bis zur nächsten Neuberechnung den alten Namen.
- Fehlertexte aus dem Feature-Modell nennen Features teils nur über ihre interne Nummer („Fläche … von Feature 3“, „Körper 0 von Feature 5“); `EdgeNotFound` nennt keine Herkunft.
- Zahlen sind auf ±1e9 mm begrenzt (`MAX_COORD_MM`): Maße, Lagen, Verschiebungen und die Eckpunkte jedes Ergebniskörpers werden beim Berechnen geprüft; größere Werte ergeben einen Fehlerstatus statt eines Absturzes. Die Prüfung sieht nur Eckpunkte; gekrümmte Flächen können bis etwa um ihren Radius darüber hinausreichen.
- Boolesche Operationen heißen „Boolesche Operation N“; die Art steht nicht im Namen.
- Der Zweig, der eine fehlgeschlagene Verschiebung zurücknimmt (`move_body`), ist nicht getestet; riesige Verschiebungen werden vorher abgelehnt (±1e9 mm).

**Oberfläche**
- Beim Ziehen am Feld „Position“ werden alle Körper in jedem Bild neu vernetzt (später Netze zwischenspeichern).
- Verschiebt man einen Körper im Eigenschaftenfenster, während eine Skizze offen ist, wird die Skizze beendet.
- Löscht man die offene Skizze, geht ihr Entwurf ohne Nachfrage verloren.
- „Löschen“ beendet die offene Skizze auch dann, wenn die Rückfrage abgebrochen wird.
- Die offene Skizze wird auch dann verworfen, wenn ihr Entwurf von dem gelöschten Feature gar nicht mehr abhängt.
- Der Hinweis in der Löschen-Rückfrage nennt nur Skizzen auf Flächen, nicht Skizzen mit Kantenkopien, die nach dem Löschen einer Booleschen Operation oder eines Verschiebens auf `EdgeNotFound` laufen.
- Die Ansicht passt sich neu ein, sobald sich die Zahl der Körper ändert; gewählte Flächen und Kanten werden nach einer Neuberechnung über ihre Arena-Nummer weitergeführt und können dann eine andere Fläche meinen.
- Ziehen am Feld „Position“ legt höchstens ein Verschieben je Zug an (es wird im selben Zug erweitert, auch wenn danach ein Leser steht).
- Fehlt dem Skizzen-Feature ein Ergebnis, nutzt die Oberfläche die gespeicherte Definition.
- **Von Hand nur teilweise geprüft:** Die frühere Annahme, die App stürze ab, war falsch: die Bildschirmsteuerung der Agenten scheiterte, weil `planora-app` keine installierte App ist. Mit `tools/gui-drive.ps1` (echte Mausklicks per PowerShell) wurden Quader, Skizze auf Fläche, Extrusion, Vereinigen, Unterdrücken, Zurückrollen, Löschen mit Rückfrage und Position ziehen von Hand geprüft. Nicht von Hand geprüft: Rotation, Fehlerfälle, Skizze mit Kantenkopie. Beobachtung: Nach „Zurückrollen“ bleibt die Umrisslinie der nicht berechneten Skizze im Bild sichtbar.

**Aktualisierung früherer Abschnitte**
- „Körper aus Skizzen sind Momentaufnahmen“ und „Keine lebende Verknüpfung“ gelten nicht mehr: Skizzen sind Features, Körper und Kantenkopien folgen Änderungen (Grenzen oben).
- „Kein Rückgängig“ und „Kein Speichern/Laden“ gelten weiter.

## Teilprojekt 6 – Speichern und Projekte
Stand: 2026-10-11. Dokumente lassen sich in Projekten speichern, als Versionen mit Zweigen verwalten und in mehreren Tabs öffnen; es gibt eine Autosicherung. Spec: `specs/2026-10-10-speichern-und-laden-design.md` (Abschnitt 11 listet, was bei der Umsetzung anders wurde).

**Handprüfung (teilweise erledigt).** Am 2026-10-11 von Hand im echten Fenster geprüft (mit `tools/gui-drive.ps1`): Projekt anlegen, Quader anlegen, Strg+S und „Speichern unter“, zweite Version mit Kommentar, Versionen-Panel, alte Version öffnen (schreibgeschützt, Menüs ausgegraut), Wiederherstellen, Fenster-× mit ungespeicherten Änderungen (Rückfrage, das Programm bleibt offen), „Speichern“ in der Rückfrage beendet das Programm und gibt die Sperre frei, Neustart und Öffnen aus dem Projekt-Panel (der Stand ist vollständig da), × ohne Änderungen schließt sofort. Nicht von Hand geprüft (nur durch automatische Tests ohne Fenster): Abzweigen mit Zweignamen-Fenster, Zweig oder Version löschen, Kommentar ändern, Ordner anlegen, umbenennen, verschieben und löschen im Projekt-Panel, Autosicherungs-Angebot nach einem Absturz, Sperre übernehmen, Schließen bei minimiertem Fenster, mehrere Tabs mit mehreren Dokumenten. Die Handprüfung fand zwei Fehler, beide behoben: Das Projekt-Panel zeigte nach „Speichern unter“ das neue Dokument erst nach „Aktualisieren“ (jetzt zählt `Workspace::library_revision` Änderungen, und das Panel liest den Baum danach neu); und der Wurzelpfad erschien mit dem Windows-Präfix `\\?\` (jetzt wird er nur in der Anzeige entfernt).

**Funktionsumfang (Grenzen)**
- Kein Zusammenführen von Zweigen, keine Cloud, kein Teilen, keine Berechtigungen.
- Jede Version ist ein vollständiger Stand (kein Delta, keine Komprimierung). Große Skizzen erzeugen große Versionsdateien.
- Keine Vorschaubilder; die Kameraansicht wird nicht gespeichert.
- Unbenannte (noch nie gespeicherte) Dokumente haben keine Autosicherung. Offene Skizzenentwürfe werden nicht mit gesichert; beim Speichern wird der Entwurf dagegen mitgenommen.
- Die Sperre eines Dokuments wird immer gemeldet und nur nach Rückfrage übernommen; ob der Prozess der Sperre noch lebt, wird nicht geprüft (dafür wäre eine neue Abhängigkeit nötig). Eine vorübergehend nicht lesbare Sperrdatei kann zu einer falschen Meldung „Sperre übernommen“ führen. Ein Speicher, dessen Sperre übernommen wurde, schreibt nichts mehr.
- Verschieben im Projektbaum nur über ein Auswahlfenster, nicht durch Ziehen.
- Umbenennen, Verschieben und Löschen verlangen geschlossene Tabs der betroffenen Dokumente (auch Ansichten alter Versionen). „Wurzel wählen“ ist nur ohne offene gespeicherte Dokumente möglich (Prüfung in `Workspace::choose_root`).
- Ein beschädigter Dokumentordner (ohne lesbare `dokument.json`) lässt sich in der Oberfläche nicht umbenennen oder löschen.
- Versionsnummern werden nie wiederverwendet (`hoechste_nummer` in `dokument.json`); Zähler und Listen haben feste Obergrenzen (Namen ≤ 80 Zeichen, Zweignamen ≤ 60, Kommentare ≤ 500, höchstens 100 000 Versionen je Dokument, Listen in einer Datei höchstens eine Million Einträge, Namenszähler „Quader n“ ab 1 000 000 ignoriert). Die Grenzen sind benannte Werte im Code und nicht einstellbar.

**Prüfung beim Laden**
- Das Format ist durch zwei Referenzdateien (`format1.json`, `format1_voll.json`) und durch verfälschte Dateien in Tests abgesichert.
- Alle Dateien werden vollständig geprüft (Typen, Ids, Verweise, Zahlen, Längen). Beim Laden sind Feature-Zahlen auf ±2e9 begrenzt: größere Werte lassen die Datei nicht laden. Beim Rechnen gilt die Grenze ±1e9 (Fehlerstatus am Feature). Bekannte Grenze: In einem konstruierten Fall kann `free_origin` den Ursprung eines neuen Körpers neben einem sehr großen Körper über 2e9 setzen; dann lehnt `encode_version` das Dokument ab, und weder Speichern noch Autosicherung funktionieren für dieses Dokument. Die Wertebereichsprüfung gilt für Skizzen und Zahlenfelder; Werte von Features zwischen 1e9 und 2e9 werden nicht abgelehnt, sondern führen beim Rechnen zu einem Fehlerstatus am Feature.
- Skizzen werden beim Laden nicht auf Abweichung gegen die gespeicherten Punkte verglichen; sie werden erst bei der Neuberechnung des Dokuments gelöst.
- Die Länge von Listen wird erst geprüft, nachdem die JSON-Bibliothek sie gelesen hat; der Speicher wird dabei nicht vorab begrenzt (nur lokale Dateien).

**Dateisystem**
- Die Prüfung gegen Verknüpfungen (Symlinks/Junctions) ist nicht gegen Wettlaufsituationen zwischen Prüfen und Schreiben gesichert (bewusst akzeptiert).
- Die Groß-/Kleinschreibung beim Verschieben in einen eigenen Unterordner wird auf Windows nicht eigens abgefangen; statt einer klaren Meldung erscheint dann der Text des Betriebssystems.
- `DocLock::acquire` ignoriert einen Fehler beim Schreiben der Sperrdatei: dann bleibt eine leere Sperrdatei und jedes Speichern meldet fälschlich, die Sperre sei übernommen worden; selten (Festplatte voll).
- `Library::kind_of` akzeptiert auch Pfade, deren erster Teil kein Projekt ist; die Oberfläche bietet nur Knoten des Baums an, daher ohne Folgen.
- Eine unlesbare Autosicherung (z. B. neueres Format) bleibt für immer liegen und wird gemeldet, aber nie überschrieben; solange sie liegt, gibt es für diesen Tab in der Sitzung keine eigene Autosicherung.

**Oberfläche**
- Die Rückfragefenster sind nicht modal: Ansicht und Browser lassen sich dahinter bedienen. Menü, Tab-Leiste und beide Panels (samt ihren Fenstern für Neuer Ordner, Umbenennen, Verschieben und Löschen) sind dagegen gesperrt, solange ein Fenster offen ist. Kommt während einer Rückfrage eine Schließanfrage, ersetzt die Beenden-Rückfrage die erste; ein Autosicherungs-Angebot hält Beenden und Schließen an, bis es beantwortet ist.
- Das Autosicherungs-Angebot hat kein „später entscheiden“. Nach erfolgreichem Strg+S bleibt ein noch offenes Angebot stehen.
- Der Punkt am Tab wird gezeichnet, nicht als Zeichen gesetzt: Die Schrift Roboto enthält das Zeichen ● (U+25CF) nicht. Ein Test prüft alle Nicht-ASCII-Zeichen der Oberflächen-Quelltexte gegen die Schrift (Ausnahmen mit ASCII-Ersatz in der Skizzenansicht).
- Schlägt „Speichern unter“ fehl, geht der eingetragene Name im Hinweis verloren (neu eintippen). Ein Dialog, der unveränderte Werte übernimmt, markiert das Dokument trotzdem als geändert (der Änderungszähler steigt auch bei einer Änderung ohne Wirkung).
- Das Versionen-Panel liest für reine Ansichten alter Versionen die Datei einmal und merkt sich das Ergebnis; Änderungen eines anderen Programms am Dokument werden dort erst nach einer Aktion oder einem Tabwechsel sichtbar. Der Zwischenspeicher kann bei ungewöhnlicher Reihenfolge (Panel nicht gezeichnet, ungespeicherter Tab aktiv) kurz veraltet sein. Eine Ansicht, deren Ausgangsversion gelöscht wird, wird nicht nachgeführt.
- Im Versionen-Panel zeigen Menüeinträge auf einer Ansicht ohne beschreibbaren Tab einen Hinweis, statt zu wirken; mehrere Menüeinträge sind dann ausgegraut.
- Die Anordnung der Panels wird nicht gespeichert; bei jedem Start gilt die Standardanordnung (Projekte, Browser, 3D-Ansicht, Eigenschaften und Versionen als Tabs). Wird der rechte Tab-Container später entfernt, könnten Kennungen im Dock veralten.
- Schreibgeschützte Tabs sperren nur die Oberfläche (Browser, Eigenschaften, Menüs „Erzeugen“/„Bearbeiten“); nicht jede Zustandsmethode in `AppState` prüft den Schreibschutz.
- Die Wahl eines Zweigs im Versionen-Panel öffnet seine Spitze als Ansicht; der Arbeitsstand des bearbeitbaren Tabs bleibt unberührt.

**Tests**
- Schwache Plantests, bewusst nicht geändert: `the_tree_is_not_reread_every_frame` (Teil der Aussage ist immer wahr). Es fehlen strengere Tests: ein auf der Platte neu angelegtes Projekt erscheint erst nach „Aktualisieren“; ein zwischengespeicherter Lesefehler; Umbenennen/Verschieben/Löschen bei Ansichten und bei anderer Groß-/Kleinschreibung; die „Ansicht“-Schalter.
- `PlanoraApp::logic` selbst (die drei Zeilen Verdrahtung des Schließablaufs) ist nicht ohne Grafikkarte testbar; getestet ist `handle_close_request`.

**Aktualisierung früherer Abschnitte**
- „Kein Speichern und Laden“ in den Abschnitten zu den Teilprojekten 1–5 gilt nicht mehr. „Kein Rückgängig“ gilt weiter.

## Teilprojekt 7 – NURBS (Freiformkurven)
Stand: 2026-10-12. Ellipsen und Splines aus Skizzen werden zu Körpern mit Freiformflächen; Boolesche Operationen gehen auch damit. Spec: `specs/2026-10-11-nurbs-design.md` (Abschnitt 13 listet, was bei der Umsetzung anders wurde). Zahlen in Millimetern.

**Handprüfung:** steht noch aus. Bisher nur durch automatische Tests ohne Fenster geprüft.

**Alte Dokumente (bewusst hingenommen)**
- Extrusion und Rotation merken sich ihr Profil nur als Nummer („Profil 0“, „Profil 1“ …). Die Profile einer Skizze sind nach der kleinsten Kurven-Nummer sortiert. Bis Teilprojekt 7 wurden Ellipsen und Splines abgelehnt und bekamen keine Profilnummer; jetzt bekommen sie eine. Das Dateiformat bleibt 1, beim Öffnen wird neu berechnet. Deshalb kann eine alte Datei jetzt ein anderes Ergebnis zeigen:
  - Ellipse vor dem Rechteck gezeichnet: Die alte „Extrusion 1“ (Profil 0 war das Rechteck) extrudiert jetzt die Ellipse.
  - Ellipse innerhalb eines Rechtecks: Die Platte bekommt jetzt ein Loch.
  - Ellipse überlappt den Rand des Rechtecks: Früher in Ordnung, jetzt werden beide Schleifen als „schneidet sich“ abgelehnt, und die Extrusion zeigt den Fehler „Profil 0 gibt es nicht“.
- Das gilt genauso für Splines, die mit Linien oder Bögen eine geschlossene Schleife bilden.
- Ein Test (`old_documents_may_extrude_a_different_profile` in `planora-feature/tests/nurbs_features.rs`) hält den ersten Fall fest, damit eine spätere Änderung bewusst geschieht. Abhilfe wäre ein dauerhafter Verweis auf das Profil statt der Nummer (eigene Aufgabe).

**Fälle, die das Programm meldet statt rechnet**
Die Meldung steht am Feature (dort mit dem Vorsatz „Der Kernel hat die Operation abgelehnt:“) und enthält den Hinweis „Bei Körpern mit Freiformflächen: <Ursache>; bitte Lage oder Maße eines Körpers leicht ändern“, wenn die Ursache aus einem Freiform-Weg des Programms stammt oder eine mehrdeutige Berührung ist; andere Fehler (etwa Ebene gegen Kegel) behalten ihren gewohnten Text. Schlägt die Volumenprobe fehl, beginnt die Meldung mit „Bei Körpern mit Freiformflächen: Das Ergebnis ist unplausibel …“. Das Dokument bleibt unverändert, abhängige Schritte werden übersprungen.
- *Berührung statt Schnitt:* Flächen, die sich nur berühren oder tangential eintauchen, werden immer abgelehnt. Zwei gleich dicke Zylinder auf derselben Achse, die sich um 0,0001 bis 0,001 mm überlappen, gelten als Berührung.
- *Gleich große Querzylinder:* Zwei gleich dicke Freiform-Walzen, deren Achsen sich kreuzen, haben zwei Schnittkurven, die sich an einer Stelle treffen, an der sich die Flächen tangential berühren. Das wird benannt abgelehnt (Test `equal_radius_crossing_reports_a_named_error_and_keeps_the_inputs`). Verschieden dicke Walzen gehen.
- *Teilweise aufeinanderliegende Freiformflächen* (etwa zwei Flächen in derselben Ebene, die sich halb überdecken): „teilweise zusammen“, abgelehnt. Liegen zwei Körper ganz deckungsgleich aufeinander (Freiformzylinder und sein einfacher Zwilling), gibt es ein Ergebnis.
- *Selbstdurchdringung:* Ein Profil, dessen Spline sich selbst oder die übrige Schleife schneidet, wird abgelehnt (`ProfileSelfIntersects`). Flächen, die sich selbst durchdringen, entstehen aus Skizzen nicht; der Schnittrechner rechnet sie nicht.
- *Flache Randberührungen:* Streift eine Fläche den Rand der anderen nur knapp, liefert der Rechner alle Schnittkurven oder lehnt mit „nahezu tangential“ ab, nie nur einen Teil. Auch lösbare Fälle (Eintauchtiefe ab 0,00001 mm) werden manchmal abgelehnt: im Test etwa 1 von 13 Fällen, bei zwei Beulenflächen rund die Hälfte. Idee: die Flächenstücke nach Größe statt nach fester Tiefe teilen und einen zusätzlichen Ausschlusstest einbauen.
- *Sehr dünne Stellen:* Entstehen beim Verschneiden Stellen, die dünner als etwa 0,00005 bis 0,0001 mm sind (Freiformfläche streift einen Quader nur knapp, ein Deckel liegt knapp unter einer Kante), meldet das Programm oft einen Fehler, obwohl die gleiche Operation mit einfachen Flächen gelingt. Ein falsches Ergebnis entsteht dabei nicht.
- *Sehr kleine Freiformkörper:* Körper mit Freiformfläche, die kleiner als etwa 0,00002 mm sind, lassen sich nicht verschneiden (Meldung „Streifschuss“). Eine Freiformkugel mit Radius 0,002 geht.
- *Flache Schnitte in eine Ellipsenwalze:* Schneidet ein Quader nur knapp (bis 0,001 mm tief) in die Wand einer extrudierten Ellipse, und zwar dort, wo die Naht der Wand liegt, kommt eine Fehlermeldung; ab 0,01 mm klappt es. An anderen Stellen klappt es ab 0,0001 mm, bei gedrehten Körpern oft nicht (Meldung, kein falsches Ergebnis).
- *Arbeitsgrenzen:* Jede Rechnung hat feste Obergrenzen (Unterteilungstiefe 12 je Richtung, höchstens 512 Startpunkte, 20 000 Schritte je Kurve, Kurvenlänge höchstens das 20-Fache der Körpergröße, 120 000 Punktproben je Flächenpaar, 4096 Kandidaten, 200 000 Flickenprüfungen). Wird eine erreicht, kommt eine eigene Meldung.

**Seltene stille Fehler (bekannte Grenze)**
- Liegt ein kurzes Stück Schnittkurve direkt neben einer schon gefundenen geschlossenen Schnittkurve, kann es fehlen, ohne dass eine Meldung kommt. Das betrifft etwa 7 von 1000 Fällen mit einer Freiform- und einer einfachen Fläche und etwa 1 von 100 Fällen mit zwei Freiformflächen; die Lücken sind 0,001 bis 0,003 mm groß. Folge: Eine Boolesche Operation mit Freiformflächen kann in seltenen Fällen ein falsches Ergebnis liefern. Die Prüfung des fertigen Körpers erkennt die meisten dieser Fälle und meldet dann einen Fehler statt eines falschen Körpers, aber nicht alle. Das Problem besteht seit der ersten Fassung des Schnittrechners.
- *Ränder zweier Freiformflächen:* Wo eine Fläche den Rand der anderen schneidet, findet der Rechner die Schnittpunkte in der Praxis zuverlässig. Bewiesen ist das nicht: Er nimmt an, dass ein kleines Flächenstück keine Falten hat, und prüft einen Teilschritt nicht vollständig.
- *Fußpunktsuche:* Die Suche nach dem nächsten Punkt auf einer Freiformfläche verfehlt für Punkte 0,05 bis 0,3 mm neben der Fläche in 1 bis 3 von 100 Stichproben einen viel näheren Punkt. Für Punkte dicht an der Fläche (die der Schnittrechner braucht) tritt das nicht auf.

**Toleranz `NURBS_FIT_TOL = 1e-5`**
- Schnittkurven mit Freiformflächen werden als Freiformkurve angenähert. Jeder Prüfpunkt der Näherung liegt höchstens 0,8 · 1e-5 mm neben beiden Flächen. Kanten mit Freiformkurven dürfen deshalb bis 1e-5 mm (`NURBS_EDGE_TOL`) neben ihren Flächen liegen, Punkte gelten beim Teilen und Vernähen bis 2e-5 mm (`NURBS_NODE_TOL`) als gleich. Einfache Körper rechnen weiter mit 1e-6 mm. Gebilde, die dünner als diese weitere Toleranz sind, kann das Programm mit Freiformflächen nicht sicher auseinanderhalten (siehe „Sehr dünne Stellen“).
- Geschlossene Schnittkurven schließen genau, sind an ihrer Naht aber nur stetig, nicht glatt.
- Bei sehr großen Koordinaten (um 1 000 000 mm) ist der Schnitt nur auf etwa 1e-8 relativ genau.

**Grenzwerte für Freiformdaten (`planora_math::nurbs`)**
Freiformkurven und -flächen werden nur mit geprüften Werten angelegt, auch beim Laden; sonst kommt ein Fehler statt eines Absturzes:
- Grad 1 bis 9 (`MAX_DEGREE`), höchstens 4096 Kontrollpunkte je Richtung (`MAX_CONTROL_POINTS`) und 262 144 je Fläche (`MAX_CONTROL_TOTAL`), höchstens 65 536 Bézier-Stücke beim Zerlegen einer Fläche (`MAX_BEZIER_PATCHES`).
- Koordinaten höchstens 1e9 mm (`MAX_COORD`), Gewichte zwischen 1e-6 und 1e6.
- Knoten höchstens 1e9 im Betrag (`MAX_KNOT_MAGNITUDE`), Parameterbereich mindestens 1e-6 lang (`MIN_KNOT_DOMAIN`), verschiedene Knoten mindestens 1e-9 des Bereichs auseinander (`MIN_KNOT_GAP_RATIO`).
- Annäherung höchstens 512 Kontrollpunkte (`MAX_FIT_CONTROL_POINTS`); Profile höchstens 4096 Sehnen je Freiformsegment (`MAX_NURBS_CHORDS`); Netz höchstens 512 Teilungen je Richtung einer Freiformfläche (`MAX_NURBS_DIVISIONS`). Ein Skizzen-Spline hat wie bisher höchstens 64 Punkte.
- Die Grenzen sind benannte Werte im Code und nicht einstellbar.

**Rechenzeiten**
- Eine Boolesche Operation mit Freiformfläche braucht im Release-Build meist etwa 0,5 bis 1 s (im Debug-Build 2 bis 7 s), vor allem für die Suche nach Kantentreffern und Fußpunkten. Bei zwei Freiformkörpern brauchen Fälle, die am Ende abgelehnt werden, bis zur Arbeitsgrenze des Schnittrechners 3 bis 5 s (Release).
- *Körper aus Splines mit vielen Punkten sind langsam:* Ein gedrehter Spline mit 39 Punkten (die Skizze erlaubt 64), mit einem Zylinder verschnitten, brauchte im Release-Build 11 bis 14 s, in der Abschlussprüfung auch 68 bis 100 s. Die Abschlussnachbesserung macht die Abstandsprüfungen beim Teilen der Flächen billiger (sichere Hüllquader der Freiformkurven statt Suche); der 11–14-s-Fall braucht jetzt knapp 4 s. Bei eigenen Messungen brauchten andere Lagen 1 bis 7 s (die langsamste war schon vorher so langsam). Im Review brauchten 24 zufällige Lagen eines gedrehten Splines mit 39 Punkten gegen einen Zylinder 0,02 bis 16,4 s (Release); gültige Ergebnisse brauchten dabei bis 11,8 Millionen Auswertungen (etwa 13 s) bei einer Grenze von 15 Millionen. Manche lösbare Lage kann deshalb mit „zu aufwendig“ enden. Die 68–100-s-Fälle der Abschlussprüfung lagen nicht vor.
- *Arbeitsgrenze:* Jede Boolesche Operation mit Freiformflächen endet spätestens nach 15 Millionen Auswertungen von Freiformkurven und -flächen (`MAX_FREEFORM_EVALUATIONS`; gezählt, nicht gestoppt, daher bei jedem Lauf gleich) mit der Meldung „Die Berechnung ist zu aufwendig …; bitte weniger Spline-Punkte oder eine einfachere Form verwenden“. Das entspricht bis etwa 16 s im Release-Build (geprüft wird an festen Stellen; die letzte Stufe läuft über die Grenze hinaus). In den eigenen Tests brauchte die aufwendigste Operation gut 6 Millionen (gemessen, bevor der Aufbau der Zwischenspeicher aus der Zählung genommen wurde); im Review brauchten gültige Ergebnisse bis 11,8 Millionen. Der Abstand zur Grenze ist also klein. Nicht gezählt werden Bézier-Flicken (Strahlschnitt) und der Aufbau dauerhafter Zwischenspeicher.
- *Fenster friert ein:* Die App rechnet im Fenster-Thread. Bis die Rechnung oder die Arbeitsgrenze fertig ist, reagiert das Fenster nicht, auch beim Öffnen eines Dokuments mit solchen Schritten. Abhilfe wäre eine Rechnung im Hintergrund (eigene Aufgabe).
- Die Suche nach dem nächsten Punkt ist für Punkte etwas abseits der Fläche (0,3 mm) bei kleinen Beulenflächen bis etwa dreimal langsamer als in einer früheren Fassung; nahe an der Fläche ist sie gleich schnell.
- Körper mit Freiformflächen werden feiner vernetzt (etwa 1,8-mal so viele Dreiecke), damit Volumen und Schwerpunkt genau genug sind.
- Mehrere Testreihen laufen wegen ihrer Dauer nur mit `cargo test --release -- --ignored`: `seeded_shallow_rim_crossings_40` (etwa 15 s), `random_pairs_are_never_wrong_and_rarely_refused` (etwa 3 bis 6 min), `a_very_large_plate_with_a_small_nurbs_hole_is_cut_again`, `a_box_cutting_a_nurbs_sphere_near_its_pole_gives_results_in_all_placements` (etwa 30 s), `hostile_tilts_do_not_panic` (etwa 30 s), `thin_features_give_valid_results_or_named_errors` (etwa 3 min), `a_spline_with_39_points_ends_within_the_real_limit` (etwa 35 s). Dazu der bekannte Netzfehler `box_on_a_rotated_analytic_cylinder_meshes_closed`, der heute fehlschlägt.

**Nicht umgesetzt (geplant für später)**
- *Gradanhebung* (Grad einer Kurve oder Fläche erhöhen): in diesem Teilprojekt nicht gebraucht; kommt mit Loft und Sweep.
- *Versatzfläche* (`offset`, Fläche im festen Abstand): nicht gebaut; sie gehört zu Teilprojekt 8 (Rundungen und Fasen).
- Kontrollpunkte bearbeiten, Import/Export, Loft und Sweep: nicht Teil dieses Teilprojekts.

**Boolesche Operationen mit Freiformflächen, weitere Grenzen**
- *Netz schmaler Streifen:* Ist ein Teil der Oberfläche schmaler als die Genauigkeit des Netzes, kann das Netz dort Lücken haben, ohne dass es gemeldet wird. Betrifft nur winzige Streifen; der Körper selbst ist richtig.
- *Voller Ring als Freiformfläche* ist nicht geprüft, weil das Programm solche Körper noch nicht erzeugt.
- In der App liegt das Volumen eines Ellipsenkörpers bei grober Netzgenauigkeit etwa 0,5 % unter dem genauen Wert.

**Netz (Vernetzung), gefunden beim Prüfen von Task 10 – eigene Aufgabe**
- Bei Ergebnissen Boolescher Operationen hat das Netz an geraden Freiformkanten manchmal winzige Lücken: im Review von Task 10 17 von 3456 Ergebnissen (0,5 %), in der Abschlussprüfung 14 von 769 (1,8 %) bei Sehnentoleranz 0,001. Also etwa 0,5 bis 2 von 100 Ergebnissen.
- Die App zeigt jetzt in der Statuszeile „Hinweis: Das Netz dieses Körpers ist an einzelnen Stellen ungenau; die Anzeige kann kleine Lücken haben.“, wenn das Netz eines Körpers nicht geschlossen ist oder eine Fläche in `Mesh::inexact_faces` steht (nur wenn keine andere Meldung ansteht). Sonst ändert sich nichts; der Körper bleibt nutzbar.
- Sehr schmale Streifen werden je nach eingestellter Netzgenauigkeit lückenhaft vernetzt, etwa bei der Grundeinstellung der App.
- Schon vorher vorhanden, ohne Freiformflächen: Ein Quader auf einem gedrehten Zylinder (vereinigt oder abgezogen) ergibt ein lückenhaftes Netz, egal wie fein. Der Test dazu steht in `known_mesh_bugs.rs` und ist vorerst abgeschaltet.
- Eine gedrehte Freiformkugel, mit einem Quader vereinigt, der sie nur hauchdünn anschneidet, lässt sich gar nicht vernetzen.
- Vorschlag: Beide Nachbarflächen einer Kante nutzen beim Vernetzen dieselben Kantenpunkte, und die App prüft das Netz eines Ergebnisses auf Lücken und meldet einen Fehler.

**Aufgeschobene Kleinigkeiten aus den Prüfungen (ohne Folgen für übliche Teile)**
- Netz: Kanten mit Freiformkurven werden gleichmäßig im Parameter geteilt. Ballen sich die Knoten einer Kurve (sehr ungleich verteilte Spline-Punkte), weicht das Netz dort um 0,04 bis 0,06 mm ab; die Fläche steht dann in `Mesh::inexact_faces`, Form und Volumen stimmen. Drehkörper aus sehr ungleich verteilten Splines können `OffSurface` melden oder sehr viele Dreiecke erzeugen. Abhilfe: Streckenzüge je Knotenabschnitt teilen.
- Netz: Eine beschnittene Freiformfläche bei sehr feiner Netzgenauigkeit verbraucht viele Dreiecke (Radius 20 bei 0,00001: 224 000 Dreiecke, 6,7 s).
- Drehen: Kurven sehr nah an der Drehachse (Abstand 0,000002 bis 0,00001) werden als „schneidet sich selbst“ abgelehnt statt mit „kreuzt die Achse“.
- Profile: Eine geschlossene Freiformkurve mit ungleichen Gewichten an ihren Enden wird abgelehnt.
- Normale: Für winzige Flächen weit vom Ursprung (Größe unter 1e-10 der Entfernung) kann die Flächennormale fälschlich null sein.
- Annäherung (`fit`): Scheitert sie, dauert das länger als nötig (Review Task 3, M5).
- Review Task 3, M2: Nicht endliche Eingaben (NaN) an `distance_to` werden nicht dort, sondern erst bei den Aufrufern abgefangen (Task 4: `implicit` liefert nie NaN). Die Punkte M4 und M8 dieses Reviews sind im Ablaufprotokoll nur als Kürzel vermerkt; ihr Inhalt ist nicht überliefert.
- Belastungstest des Schnittrechners (`random_pairs_are_never_wrong_and_rarely_refused`): Er zählt eine Schnittkurve erst als „falsch“, wenn sie mehr als 0,05 mm neben der Referenz liegt. Den bekannten stillen Wegfall kurzer Äste (Lücken 0,001 bis 0,003 mm) kann er deshalb nicht erkennen; „kein falsches Ergebnis“ heißt dort nur: keine groben Fehler.
- Drehkörper aus ungleichmäßigen Splines: Bei den Messungen zur Arbeitsgrenze (Spline mit 39 zufälligen Punkten, gedreht, gegen einen Zylinder) endeten mehrere Lagen mit „Eine Kante liegt nicht auf der Freiformfläche“ und eine schon an der Eingangsprüfung des Körpers (gleiche Ursache wie die Netzabweichung bei geballten Knoten, oben).
- Fußpunktsuche: In etwa 1 von 4600 Fällen verfehlt sie den nächsten Punkt um bis 0,01 mm; mit Startwert kann das Ergebnis bis 0,1 mm von dem ohne Startwert abweichen. Die Nullstellensuche einer Kurve gegen eine Freiformfläche kostet etwa 8 ms je Aufruf, auch bei kleinen Flächen.
- Einordnen (Review Task 10): Das Randpolygon einer Deckelfläche weicht an einer Spline-Kante um mehr als 0,0001 mm von der echten Kante ab; ein Punkt 0,0001 mm außerhalb der Kante kann deshalb als „auf dem Rand“ oder „innen“ gelten (1 von 34 000 Stichproben).
- Tests: Vier Testprogramme brauchen im Debug-Build je 25 bis 31 s.

## Aufräumen / Qualität
- `rust-version = "1.88"` ist für die GUI-Crates ungenau (egui 0.36 braucht 1.95).
- `eframe` mit Standard-Features baut auch den `glow`-Renderer mit.
- CI baut `cargo-deny` bei jedem Lauf neu.
- Tests: Pan-/Orbit-Vorzeichen, Auswahl vor Hover, Vertex-Größen, `UnsupportedSegment`, Torus bei Uhrzeigersinn-Kette.
- Auswahl: CPU-Raycast läuft bei jeder Mausbewegung über alle Dreiecke (später Hüllkörper/Raumindex).
- Hover-Änderung baut die ganze Szene neu und lädt sie hoch (später nur Farben aktualisieren).
- Browser: lange Namen werden abgeschnitten; Klick im Browser setzt keine Flächenauswahl.
