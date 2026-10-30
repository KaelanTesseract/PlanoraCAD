# Offene Punkte nach Teilprojekt 1–10

Stand: 2026-10-10 (Teilprojekt 5 siehe eigener Abschnitt weiter unten). Teilprojekt 1–4: 2026-10-09. Ergebnis der Abschlussprüfungen; bewusst nicht in Teilprojekt 1–4 behoben.

## Entscheidung des Projekteigners nötig
- **Markenrecherche** für „PlanoraCAD“ (DPMA, EUIPO, WIPO, Klasse 9) vor einer Veröffentlichung.
- **Linux:** App-Start dort noch nicht geprüft (CI baut und testet nur).

## Beobachten
- **`ttf-parser 0.25.1`** (RUSTSEC-2026-0192, „unmaintained“, keine Sicherheitslücke) wird nur auf dem Linux/Wayland-Pfad gezogen (winit 0.30 → sctk-adwaita 0.10 → ab_glyph) und ist in `deny.toml` begründet ignoriert. Erneut prüfen, sobald winit 0.31 stabil und übernommen ist (nutzt sctk-adwaita ≥ 0.12, das `skrifa` verwenden kann); dann den Eintrag entfernen.

## Bekannte Grenzen
- Roboto 2.138 enthält die Zeichen U+23F4 ⏴ / U+23F5 ⏵ (von `egui` für Untermenü-Pfeile und Überlauf-Pfeile genutzt; `egui_tiles` ist seit Etappe B entfernt) und U+2192 → nicht. Heute ohne Folgen (keine Untermenüs, die Tab-Leiste rollt ohne Pfeile), würde aber als leere Kästchen erscheinen. Lösung bei Bedarf: Pfeile als Formen zeichnen oder eine Ersatzschrift ergänzen.

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
- Ein Ergebnis ersetzt beide Ausgangskörper. (Damals: kein Rückgängig; seit dem Teilprojekt „Rückgängig und Wiederholen“ holt ein Rückgängig beide Ausgangskörper zurück.)
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
- (Damals: kein Rückgängig, kein Speichern/Laden. Beides gibt es inzwischen, siehe Teilprojekt 6 und „Teilprojekt: Rückgängig und Wiederholen“.)
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
- Hinweise erscheinen in der Statuszeile und im Fußfeld der Skizze (bis Etappe B: im Skizzenfenster) ebenfalls rot (keine Unterscheidung Hinweis/Fehler).
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
- (Damals auch: kein Speichern und Laden, kein Rückgängig; beides gibt es inzwischen.) Keine benannten Parameter oder Gleichungen, keine Muster- oder Spiegel-Features.
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
- „Kein Speichern/Laden“ galt damals weiter (seit Teilprojekt 6 erledigt); „Kein Rückgängig“ ebenfalls (seit dem Teilprojekt „Rückgängig und Wiederholen“ erledigt).

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
- Das Ein-/Ausblenden von Projekten und Versionen sowie der gewählte Reiter rechts (Eigenschaften oder Versionen) wird nicht gespeichert; bei jedem Start sind beide Felder sichtbar (seit Etappe B feste schwebende Felder statt Dock).
- Schreibgeschützte Tabs sperren nur die Oberfläche (Browser, Eigenschaften, Menüs „Erzeugen“/„Bearbeiten“); nicht jede Zustandsmethode in `AppState` prüft den Schreibschutz.
- Die Wahl eines Zweigs im Versionen-Panel öffnet seine Spitze als Ansicht; der Arbeitsstand des bearbeitbaren Tabs bleibt unberührt.

**Tests**
- Schwache Plantests, bewusst nicht geändert: `the_tree_is_not_reread_every_frame` (Teil der Aussage ist immer wahr). Es fehlen strengere Tests: ein auf der Platte neu angelegtes Projekt erscheint erst nach „Aktualisieren“; ein zwischengespeicherter Lesefehler; Umbenennen/Verschieben/Löschen bei Ansichten und bei anderer Groß-/Kleinschreibung; die „Ansicht“-Schalter.
- `PlanoraApp::logic` selbst (die drei Zeilen Verdrahtung des Schließablaufs) ist nicht ohne Grafikkarte testbar; getestet ist `handle_close_request`.

**Aktualisierung früherer Abschnitte**
- „Kein Speichern und Laden“ in den Abschnitten zu den Teilprojekten 1–5 gilt nicht mehr. „Kein Rückgängig“ galt damals weiter und gilt seit dem Teilprojekt „Rückgängig und Wiederholen“ nicht mehr.

## Teilprojekt 7 – NURBS (Freiformkurven)
Stand: 2026-10-12. Ellipsen und Splines aus Skizzen werden zu Körpern mit Freiformflächen; Boolesche Operationen gehen auch damit. Spec: `specs/2026-10-11-nurbs-design.md` (Abschnitt 13 listet, was bei der Umsetzung anders wurde). Zahlen in Millimetern.

**Handprüfung (teilweise erledigt).** Am 2026-10-12 von Hand im echten Fenster geprüft (Release-Build, vorübergehende Benutzerordner): Ellipse auf der XY-Ebene skizzieren und extrudieren (Profilfläche 633,30 mm², Volumen etwa 6327 mm³, die Fläche heißt im Eigenschaftenfenster „Freiformfläche“), Zylinder anlegen und über „Position“ verschieben, Abziehen (Zylinder minus Ellipsenkörper: 7 Flächen, 15 Kanten, 16 110 mm³), Mittelpunkt der Ellipse in der Skizze ziehen (der Körper rechnet sich neu: 16 341,67 mm³), in ein Projekt speichern, Programm hart beenden, neu starten, die zurückgebliebene Sperre mit „Sperre übernehmen“ übernehmen und das Dokument öffnen (gleiche Form, gleiches Volumen, gleiche 7 Flächen). Ein Spline in einer Skizze auf der XZ-Ebene, mit einer Linie geschlossen, die den Spline kreuzt: Die Oberfläche meldet richtig „Nicht verwendbar: Das Profil schneidet oder berührt sich selbst oder ein anderes Profil“. Nicht von Hand geprüft (nur durch automatische Tests ohne Fenster): Extrusion oder Drehung eines gültigen Splines im Fenster, Skizze auf einer Körperfläche, Ändern der Halbachsen-Maße, Arbeitsgrenze und Einfrieren des Fensters, der Netz-Hinweis in der Statuszeile.

**Oberfläche**
- Die Ellipse hat noch keine Maß-Eingabe in der Skizzen-Oberfläche: Radius, Durchmesser und Abstand sind bei einer Ellipse ausgegraut. Die Abhängigkeiten für die Halbachsen gibt es im Solver (und in gespeicherten Dateien), aber keine Schaltfläche dafür.

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
- Mehrere Testreihen laufen wegen ihrer Dauer nur mit `cargo test --release -- --ignored`: `seeded_shallow_rim_crossings_40` (etwa 15 s), `random_pairs_are_never_wrong_and_rarely_refused` (etwa 3 bis 6 min), `a_very_large_plate_with_a_small_nurbs_hole_is_cut_again`, `a_box_cutting_a_nurbs_sphere_near_its_pole_gives_results_in_all_placements` (etwa 30 s), `hostile_tilts_do_not_panic` (etwa 30 s), `thin_features_give_valid_results_or_named_errors` (etwa 3 min), `a_spline_with_39_points_ends_within_the_real_limit` (etwa 35 s). Dazu der Netz-Dauertest `tilted_boxes_on_a_nurbs_cylinder_mesh_closed` (etwa 13 min).

**Nicht umgesetzt (geplant für später)**
- *Gradanhebung* (Grad einer Kurve oder Fläche erhöhen): in diesem Teilprojekt nicht gebraucht; kommt mit Loft und Sweep.
- *Versatzfläche* (`offset`, Fläche im festen Abstand): nicht gebaut; sie gehört zu Teilprojekt 8 (Rundungen und Fasen).
- Kontrollpunkte bearbeiten, Import/Export, Loft und Sweep: nicht Teil dieses Teilprojekts.

**Boolesche Operationen mit Freiformflächen, weitere Grenzen**
- *Netz schmaler Streifen:* Ist ein Teil der Oberfläche schmaler als die Genauigkeit des Netzes, kann das Netz dort Lücken haben, ohne dass es gemeldet wird. Betrifft nur winzige Streifen; der Körper selbst ist richtig.
- *Voller Ring als Freiformfläche* ist nicht geprüft, weil das Programm solche Körper noch nicht erzeugt.
- In der App liegt das Volumen eines Ellipsenkörpers bei grober Netzgenauigkeit etwa 0,5 % unter dem genauen Wert.

**Netz (Vernetzung), gefunden beim Prüfen von Task 10 – Stand nach der Netz-Nachbesserung**
- *Behoben, Quader auf der Stirnfläche eines Zylinders (ohne Freiformflächen; häufig: in einer Stichprobe des Reviews 93 % solcher Lagen):* Der Randkreis des Mantels war dort in mehrere Kanten geteilt. Weil der Mantel trotzdem seine volle Fläche hatte, galt er als „unbeschnitten“ und bekam ein gleichmäßiges Raster, dessen Randpunkte nicht zu den Teilpunkten der Nachbarfläche passten. Jetzt nimmt eine solche Fläche das Raster nur noch, wenn seine Randpunkte genau die gemeinsamen Kantenpunkte sind. Sonst wird ein Zylinder- oder Kegelmantel als Streifen zwischen seinen beiden Randkreisen vernetzt, genau mit deren Teilpunkten (etwa so viele Dreiecke wie vorher: Zylinder r = 300 mit Quader auf der Stirnfläche 502, 1554, 2894 Dreiecke bei 0,1, 0,01, 0,001 statt vorher 501, 1553, 2893 bei offenem Netz; ein erster Versuch über die beschnittene Vernetzung hatte bis 95-mal so viele). In 150 zufälligen Lagen von Quadern auf Zylinder- und Kegelstirnflächen waren vorher alle Netze offen, jetzt alle geschlossen, mit im Mittel 0,3 % und höchstens 3 % mehr Dreiecken. Andere Flächen (Kugel, Ring) und Mäntel, deren Rand die Randkreise abseits der Naht verlässt (etwa ein haarfeiner Schlitz quer durch den Mantel), werden in diesem Fall wie beschnittene Flächen vernetzt. An der Kegelspitze bekommt jeder Abschnitt einen eigenen Spitzenpunkt mit eigener Normale (wie beim Raster). Alle Netze, die vorher geschlossen waren, sind Punkt für Punkt gleich geblieben (geprüft an Grundkörpern bei fünf Netzgenauigkeiten und 200 zufälligen Booleschen Operationen). Der Test `known_mesh_bugs.rs` läuft wieder mit (Netzgenauigkeit 0,05, 0,002 und 0,0005).
- *Behoben, „Flosse“ an Freiformflächen:* Wurde ein Freiformmantel von einer Quaderebene geschnitten, konnten Mantel und Ebene über einem kurzen Bogenstück dasselbe Stück zwischen Bogen und Sehne abdecken: Der Mantel hatte dort nur Dreiecke aus Randpunkten, die genau in der Ebene lagen. Dann hingen vier Dreiecke an einer Kante, und das Netz galt als offen. Gemessen vor der Behebung: 5 von 6187 Ergebnissen (etwa 0,1 %; gedrehte Quader gegen einen Freiformzylinder r = 3, Netzgenauigkeit 0,001 und 0,002; eine frühere Untersuchung mit anderer Lagenreihe kam auf etwa 0,25 %). Jetzt bekommt ein solches Dreieck einen Punkt auf der Fläche in seiner Mitte. Danach: 0 von 6187, im Dauertest `tilted_boxes_on_a_nurbs_cylinder_mesh_closed` 0 von 688. Die Zahlen früherer Prüfungen (0,5 bis 1,8 %) ließen sich nicht nachstellen, ihre Testreihen lagen nicht mehr vor.
- *Jetzt eine Meldung statt eines unvernetzbaren Körpers:* Eine Freiformkugel, die ein Quader nur hauchdünn (etwa 0,00002 bis 0,00003 mm tief) genau über ihrer Naht anschneidet, ergab einen „gültigen“ Körper, der sich nicht vernetzen ließ: Die Naht blieb ungeteilt, und der Schnittkreis lag als Loch quer über ihr. Die Prüfung des Körpers erkennt das jetzt („Ein Loch der Fläche … reicht über die Naht ihrer Trägerfläche“), die Boolesche Operation endet mit einer Meldung. Die eigentliche Ursache (die Kreuzung mit der Naht wird bei so flachen Schnitten übersehen) ist eine eigene Aufgabe im Kernel. Die Prüfung ist bewusst eng: Sie sucht nur Löcher, die über eine Naht reichen, keine anderen Kreuzungen von Randschleifen. Ihr Spielraum ist ein fester Bruchteil des Umfangs; bei Radien unter etwa 0,16 mm ist er kleiner als die Modelltoleranz (bisher ohne beobachtete Folgen).
- *Sehr schmale Streifen:* Die gemeldeten Lücken an schmalen Streifen von Freiformflächen ließen sich nicht nachstellen. Ohne Freiformflächen gibt es aber Ähnliches: Unter 200 zufälligen Booleschen Operationen mit Quader, Zylinder, Kugel und Kegel ließen sich zwei bei grober Netzgenauigkeit (0,05 und 0,1) gar nicht vernetzen (Meldung „Triangulation fehlgeschlagen“ an einem sehr schmalen Mantelstreifen), eine dritte hatte bei 0,1 bis 0,01 eine Flosse wie oben (an einer Kugel). Ab 0,02 bzw. 0,002 war alles geschlossen. Die Flossen-Abhilfe gilt bewusst nur für Freiformflächen, damit die Netze rein analytischer Körper unverändert bleiben.
- Die App zeigt in der Statuszeile „Hinweis: Das Netz dieses Körpers ist an einzelnen Stellen ungenau; die Anzeige kann kleine Lücken haben.“, wenn das Netz eines Körpers nicht geschlossen ist oder eine Fläche in `Mesh::inexact_faces` steht (nur wenn keine andere Meldung ansteht). Sonst ändert sich nichts; der Körper bleibt nutzbar.
- Große beschnittene Mäntel werden weiter so fein vernetzt wie vor der Nachbesserung, mit einem gleichmäßigen Raster im Inneren: Ein Zylinder r = 300, h = 500 mit einem schmalen Schlitz über der Naht hat bei 0,01 etwa 79 900 Dreiecke, ohne Schlitz etwa 1 550. Das war schon vorher so (unverändert, eigene Aufgabe, falls es stört).
- Prüfhilfe: `check_face_boundaries` prüft, dass der Rand jeder Fläche genau aus ihren Kantenpunkten besteht. Die Tests rufen sie auf; `tessellate` selbst nicht, auch nicht im Debug-Build, weil sie dort so lange wie das Vernetzen selbst oder länger dauert (gemessen: Lochplatte +120 bis +145 %, Kugel r = 30 bei 0,001 +820 %). Flossen erkennt diese Prüfung nicht, dafür gibt es `is_watertight`.
- Netz-Feinheit von Freiformflächen: Durch die Flossen-Abhilfe haben Ergebnisse mit Freiformflächen etwas mehr Dreiecke (im Review im Mittel etwa 0,06 % mehr, bei sehr kleinen Ergebnissen bis doppelt so viele; eigene Messung an 20 Ergebnissen 0 bis 9 %); die Rechenzeit des Netzes blieb fast gleich (etwa 1 bis 2 % mehr).

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

## Teilprojekt 8 – Rundungen und Fasen
Stand: 2026-10-14. Kanten lassen sich runden (fester oder variabler Radius) und fasen (gleicher Abstand, zwei Abstände, Abstand und Winkel), auch an Freiformflächen und mit Tangentenkette; Ecken nur in den unten genannten Fällen. Spec: `specs/2026-10-13-rundungen-und-fasen-design.md` (Abschnitt 12 listet, was bei der Umsetzung anders wurde). Zahlen in Millimetern.

**Handprüfung (teilweise erledigt).** Am 2026-10-14 von Hand im echten Fenster geprüft (Release-Build, vorübergehende Benutzerordner): Quader 10 × 10 × 10 anlegen, „Verrunden …“ mit einer angeklickten Kante (orange markiert, „Kleinstes Maß“ sichtbar), Radius 2 (Volumen im Fenster 990,61 mm³; das ist der Netzwert, rechnerisch 991,4 mm³), Umschalt + Klick auf weitere Kanten (die Tangentenkette nahm 5 Kanten mit), Radius 1 wurde ehrlich abgelehnt (Bogen und Rundung ergäben einen entarteten Ring), Radius 0,5 ging durch (13 Flächen). „Rundung 1“ im Baum doppelklicken und den Radius ändern (die Folge-Rundung rechnet mit), auf variablen Radius umstellen und einen Punkt hinzufügen (sichtbare Taille; die Folge-Rundung meldete „zu groß für die Fläche Rundung 1“ in Statuszeile und Baum), zurück auf konstant. Fase „Abstand und Winkel“ mit dem neuen Feld „Fläche 1“: 1 mm / 30° auf der Kette wurde als „zu groß für die Fläche Boden“ abgelehnt (ob berechtigt oder zu streng, ist offen), 0,4 mm / 45° ging durch (18 Flächen). Radius 20: Fehler „zu groß für die Fläche Wand 2“, Folgefeatures „übersprungen“, Körper unverändert. Projekt anlegen, speichern, Programm hart beenden, neu starten, Sperre übernehmen, laden: gleicher Baum, gleiche Form, gleiche Werte (990,12 mm³, 576,28 mm², 18 Flächen, 39 Kanten). Nicht von Hand geprüft: Einstellung „zwei Abstände“, Kugelecke mit drei Kanten an einer Ecke ohne Kette, konkave Kanten, Rundungen auf Freiformflächen, Datei mit Format 2 in einem älteren Programm. Auffällig: Die Namen im Baum sind in der schmalen Spalte abgeschnitten („Rundu“); das ist nur Optik.

**Was nicht geht (wird mit einer Meldung abgelehnt)**
- Körper mit Hohlräumen oder mehreren getrennten Teilen. Das Programm kann dort nicht sicher prüfen, ob die Rundung durch den Hohlraum läuft.
- Ecken: Zwei gerundete Kanten an einer Ecke laufen nur dann ineinander (Gehrung), wenn ihre Seiten spiegelgleich liegen. Drei Rundungen mit gleichem Radius bekommen ein Kugelstück, jetzt für beliebige drei Ebenen (nicht nur rechtwinklig). Abgelehnt werden: Ecken mit variablem Radius oder an Freiformflächen, nach innen springende oder gemischte Ecken, drei Fasen an einer Ecke, zwei Fasen oder zwei Rundungen ohne spiegelgleiche Flächen an einer schiefen Ecke, sehr spitze Gehrungen (etwa 4 Grad). Die Stile „Setback“ und „glatte Ecke“ kommen später.
- Abschlüsse: Eine Rundung endet an der dritten Fläche nur dann, wenn der Schnitt mit dieser Fläche unterstützt ist. Das fehlt zum Beispiel an einer gewölbten Wand ohne Tangentenkette. Rundungen an Freiformflächen und variable Rundungen enden nur an Ebenen senkrecht zur Kante, an Nähten, glatten Übergängen und Ecken.
- Zu kleine Maße: unter 0,0001 mm (bei Freiformflächen oder variablem Radius unter 0,002 mm). Fasenwinkel nur von 1° bis 179°. Der Hinweis zum Mindestmaß erscheint im Fenster erst, nachdem eine Kante gewählt ist.
- Kanten ohne Rundungsmöglichkeit, je mit eigener Meldung: Naht eines Zylinders, fast glatt anschließende Flächen (etwa ein Langloch ohne Tangentenkette), Schneiden, Flächen, die enger gekrümmt sind als der Radius, Kugelbahnen zu nahe an der Drehachse.
- Nicht Teil: Rundung ganzer Flächen, Loft, Sweep, Export von Rundungen.

**Abgelehnte Anfragen.** Ziel ist, lieber abzulehnen, als einen falschen Körper zu liefern. In den Zufallstests der ersten Stufe (einzelne Kanten) wurden abgelehnt: Quader 9 %, Prismen 20 %, L- und Langloch-Profile 61 bis 68 %, Zylinder 69 %, Drehkörper 77 %, Kegel 84 %. In der zweiten Stufe (mehrere Kanten und Ecken, absichtlich gern an Nachbarkanten gewählt): Quader 44 %, Prismen 81 %, Körper mit Löchern 66 %, gestufte Quader 84 %. Mit den späteren Stufen sanken die Quoten bei Freiformkanten und variablem Radius deutlich: bei variablem Radius wurden von 400 Fällen 383 gerechnet, Ellipsenränder 118 von 140, Ketten aus Freiformkanten 50 von 60. In keiner Testreihe kam ein falscher Körper durch. Die Abschlussprüfung fand aber einen Fall, den keine Reihe enthielt (Kugeltasche, siehe „Prüfung Fläche gegen Fläche“); die Reihen zeigen also, dass die bekannten Fälle stimmen, nicht, dass es keine Lücke mehr gibt.

**Prüfung Fläche gegen Fläche (Korrekturwelle nach der Abschlussprüfung, 2026-10-14).** Die Abschlussprüfung fand einen falschen Körper, den keine Testreihe enthielt: Würfel 10 mit einer Kugeltasche (Radius 1,2), die oben offen ist und sich von innen bis in die Rundung einer senkrechten Kante wölbt. Keine Kante der Tasche traf die neue Fläche, deshalb sah keine Prüfung die Durchdringung; die Rundungsfläche lief durch den Hohlraum. Jetzt wird jede Fläche des Ergebnisses, die einer neuen Rundungs-, Fasen- oder Eckfläche nahekommt, mit ihr geschnitten; neue Flächen auch untereinander. Lässt sich der Schnitt nicht genau rechnen (etwa Kugel und Zylinder mit verschiedenen Achsen), versucht das Programm zu zeigen, dass die beiden Flächen sicher auseinander liegen (Abstand zur Kugelmitte, Abstand zur gemeinsamen Achsrichtung, Lage zu einer Ebene, schrittweise Teilung beider Flächen in kleine Stücke). Gelingt das nicht, kommt die Meldung „… kommt der neuen Fläche … nahe; ob sie sich durchdringen, lässt sich nicht sicher prüfen“.
- Wirkung auf die Testreihen: unverändert in `blend_stage2`, `blend_stage3`, `blend_corners_general` (284 von 300), `blend_mixed`, `blend_corner_volume` und der Freiformreihe. In `blend_notch` (1000 Fälle, Aussparungen von unten) jetzt 794 statt 799 gerechnet; die 5 neuen Ablehnungen waren nachgeprüft richtige Körper, also unnötige Ablehnungen. Neue Reihe mit 300 zufälligen Kugeltaschen an der Kante (`blend_face_crossing`): 238 gerechnet und gegen die Geometrie geprüft, 62 abgelehnt, davon 1 als „nicht sicher prüfbar“ (eine echte Durchdringung). Ohne die neue Prüfung kam in dieser Reihe ein falscher Körper durch (Fall 202).
- Was weiter nicht geprüft wird: Bei zwei Flächen, die schon eine Ecke oder Kante gemeinsam haben, sucht das Programm nur dann einen weiteren Schnitt, wenn es ihn genau rechnen kann; bei Freiformflächen, die ganz auf einer Seite einer benachbarten Ebene liegen, wird ein bloßes Berühren nicht gesucht. Die beiden Flächen direkt an der gerundeten Kante werden nicht gegen ihre Rundung geschnitten (sie berühren sie längs der Berührkurve; dafür gibt es die Krümmungsprüfungen). Eine Fläche, die ganz im Hohlraum einer Tasche läge, ohne sie zu berühren, findet diese Prüfung nicht; dafür müssten Kanten durch die Tasche laufen, und das sehen die älteren Prüfungen.
- Im Zuge geschlossen: Eine Kante, die ganz auf der Trägerfläche der Rundung liegt, wurde nur an ihrer Mitte befragt; jetzt an jedem Stück zwischen ihren Kreuzungen mit dem Rand der Rundungsfläche.
- Aufwand: Im Debug-Lauf kostet die Prüfung bei `blend_variable` etwa 2 s Rechenzeit (über alle Fälle); die Laufzeit des Programms blieb unter 30 s (zuletzt 22,6 s).

**Boolesche Operationen danach.** Vereinigen, Abziehen und Schneiden mit einem gerundeten Körper werden oft abgelehnt: Die bekannten Grenzen bei Kegel- und Ringflächen (Teilprojekt 3) gelten weiter, und eine Rundung erzeugt solche Flächen. Es kommt dann eine Meldung am Schritt, kein falscher Körper.

**Eine Fläche je Kante.** Entlang einer glatten Kette bekommt jede Kante ihre eigene Rundungsfläche (Entscheidung P3); die Flächen werden nicht zusammengefasst. Die Rollen heißen „Rundung N“ bzw. „Fase N“; die Zählung beginnt in jedem Schritt bei 1. Doppelt gewählte Kanten werden nicht gemeldet, sondern einmal gerechnet.

**Volumenprobe (P2).** Nach jeder Rundung vergleicht das Programm das Volumen vorher und nachher mit einer Schätzung. Der Spielraum beträgt 50 % der Schätzung bei einheitlichen Rundungen und 20 % bei gemischten (Außen- und Innenkanten zusammen). Das fängt grobe Fehler ab, aber nicht alles: Fehlt in einem gemischten Fall nur die kleinere Art (bis etwa 20 % der Gesamtmenge), geht das durch die Probe. Die eigentliche Sicherheit kommt aus den geometrischen Prüfungen (Körper gültig, keine Kante läuft durch eine neue Fläche, Flächen in der Nähe einer neuen Fläche werden mit ihr geschnitten, Löcher und Nachbarflächen berücksichtigt); die Probe ist ein zusätzliches Netz. Bei Ecken, an denen nur ein Teil der Kanten gerundet ist, liegt die Schätzung 1 bis 3 % daneben.

**Bildschirmnetz**
- Bei steilen variablen Radien kann eine ebene Fläche, die an eine steile Freiformkurve grenzt, im Netz kleine Lücken haben (in den Tests 5 von etwa 500 Fällen, rund 1 %). Der Körper selbst ist richtig (Geometrie, Gültigkeit und Punktprobe sind in Ordnung); die Statuszeile zeigt den Hinweis „Netz ungenau“. Die Ursache liegt im Dreiecksnetz für ebene Flächen, das in diesem Teilprojekt nicht verändert wurde. Mögliche spätere Aufgabe.
- Bei Freiformflächen ist das Netz bei grober Sehne (1e-3) bis zu einigen Prozent im Volumen ungenau (Zylinder 0,84 %, Torus 2,5 %); bei Sehne 1e-4 stimmt es. Die schnellen Tests prüfen mit entsprechend weiteren Schranken, die langen Reihen streng.
- Sehr kleine Radien (um 0,002 mm) erzeugen ebenfalls ungenaue Netze (90 Fälle in den Tests).

**Genauigkeit**
- Die Querschnitte der Rundung folgen den beiden Nachbarflächen, nicht der gespeicherten Kantenkurve (Spec Abschnitt 12).
- Die Naht zwischen zwei Rundungsflächen einer Kette ist nur dann glatt, wenn der Radius an dieser Stelle von beiden Seiten dieselbe Steigung hat.
- Bei variablem Radius an Ketten aus Kreisbögen rechnet das Programm die Längen mit 48 Sehnen je Kreis; dadurch sind die Radien gegenüber der wahren Bogenlänge bis zu 7 · 10⁻⁴ gestaucht. Bei Ketten aus Freiformkanten sind die Längen genau (Gauß-Bogenlänge).
- Die Volumenschätzung des Kernels liegt bei steilen variablen Radien bis zu 1,5 · 10⁻³ daneben, bei stark gewölbten Querschnitten deutlich mehr (Kugel 11 %, Torus 7 %); beides liegt im Spielraum der Probe.
- Ein Zylinder aus Freiformflächen lässt Rundungen bis 0,78 · Radius zu, sein analytischer Zwilling lehnt früher ab (Inkonsistenz, ohne Folgen für die Richtigkeit).
- An einigen Lochrändern gibt es ein Gebiet von Radien, in dem das Zusammensetzen der Flächen scheitert (Beispiel: Loch 2,589 × 1,128, Radius 0,356 bis 0,435): Meldung, kein falscher Körper. Ein Radius von etwa 0,002 mm an geschlossenen Ketten wird als „zu groß für den Deckel“ gemeldet, obwohl die Ursache eine andere ist.

**Bedienung**
- Die Kantenverweise im Schritt werden nach einer Neuberechnung nicht zurückgeschrieben. „Fläche 1“ wird über den Flächennamen gewählt und wirkt bei zwei gleich benannten Flächen nicht. Wählt man in einem Schritt mehrere Kanten, geht ein „Fläche 1“ einer später gewählten Kante still verloren, wenn die Kante schon in der Kette einer früheren liegt.
- Im Fenster geht beim Wechsel der Art oder beim Hinzufügen eines Radienpunkts mit ungültigem Text die Eingabe verloren; ein Radienpunkt außerhalb von 0 bis 100 % wird erst von der Berechnung gemeldet. Das Umschalten der Tangentenkette wirkt erst beim nächsten Klick auf eine Kante.
- Meldungen des Kerns schreiben Dezimalzahlen mit Punkt, die Oberfläche mit Komma.
- Kanten, die zwischen 0,5° und 2° abknicken (grobe Vielecke), zählen weder als glatt noch als fast glatt; die Meldung rät dann fälschlich zur Tangentenkette.
- Die Prüfung auf Durchstoßen wächst bei sehr vielen Kanten quadratisch im Aufwand; eine Auswahl hat höchstens 256 Kanten (mit Kette); mehr werden schon vor der Suche der Kanten abgelehnt, auch aus Dateien. Ein variabler Radius hat höchstens 64 Punkte; im Fenster fügt „Punkt hinzufügen“ dann keinen mehr ein und meldet das.

**Dateiformat (P7).** Dokumente mit Rundung oder Fase werden im Dateiformat 2 geschrieben, alle anderen weiter im Format 1. Alte Programme lesen Format 1 weiter und lehnen Format 2 als „zu neu“ ab. Der Test `newer_files_with_changed_structure_report_too_new` nutzt jetzt `3` als „zu neu“ (mechanische Anpassung, mit Kommentar). `dokument.json` und die Autosave-Datei behalten ihre Struktur (`format: 1`). Fehlende Felder in Rundungsschritten melden die rohe Meldung des Dateiformats, wie bei den anderen Schritten.

**Laufzeiten und Testreihen**
- Teststand am Ende von Teilprojekt 8: 1179 bestanden, 0 fehlgeschlagen, 27 ignoriert in 153 Testprogrammen, Laufzeit etwa 7 Minuten mit Übersetzung (`cargo test --workspace`, Debug).
- Teststand nach der Korrekturwelle: 1190 bestanden, 0 fehlgeschlagen, 28 ignoriert in 154 Testprogrammen (`cargo test --workspace`, Debug). Die langen Reihen im Release (`blend_stage2`, `blend_stage3`, `blend_corners_general`, `blend_notch`, `blend_mixed`, `blend_corner_volume`, `blend_freeform_series`, `blend_face_crossing`) liefen einmal nacheinander grün.
- Längste Testprogramme im Debug-Build: `blend_variable` etwa 21 s (Grenze 30 s; auf einem langsamen Rechner knapp), `blend_freeform_volume` etwa 25 s, `blend_freeform` etwa 14 s.
- Lange Reihen laufen nur mit `cargo test --release -- --ignored`: `blend_stage2` (200 Fälle) etwa 40 s, `blend_stage3` etwa 425 s (feine Tests 30 bis 50 s, 100 Zylinder etwa 340 s), `blend_freeform_volume` etwa 263 s, `blend_corners_general` (300 Fälle: 284 gerechnet und gegen die Formel richtig, 16 mit Meldung abgelehnt) sowie `blend_corner_volume`, `blend_notch`, `blend_mixed`. Sie liefen zuletzt in Aufgabe 17 grün; in Aufgabe 20 wurden sie nicht erneut gestartet.
- Lizenzen: keine neue Abhängigkeit, `cargo deny check` sauber.

## Teilprojekt 9 – Netz-Austausch
Stand: 2026-10-14. Körper lassen sich als STL, OBJ, 3MF und glTF (.glb) ausgeben (Menü „Datei“ → „Exportieren …“, Strg+Umschalt+E). STL und OBJ lassen sich als „Netz-Körper“ einlesen („Datei“ → „Netz importieren …“). Spec: `specs/2026-10-14-netz-austausch-design.md` (Abschnitt 12 listet, was bei der Umsetzung anders wurde). Plan: `plans/2026-10-14-netz-austausch.md`. Zahlen in Millimetern.

**Handprüfung (teilweise erledigt).** Am 2026-10-14 von Hand im echten Fenster geprüft (Release-Build, vorübergehende Benutzerordner): Quader 40 × 30 × 20 und Kugel r = 20 anlegen; „Datei → Exportieren …“ (Fenster über die ganze Breite, drei Karten gleich hoch, „98 922 Dreiecke, ca. 4,9 MB“ bei „Fein“; ob der Fortschrittsbalken kurz erschien, wurde nicht beobachtet, die Berechnung war beim Bildaufnehmen schon fertig); STL, 3MF und glTF geschrieben (STL 4 946 184 Bytes = 84 + 50 × 98 922, 3MF 7,0 MB gegen Schätzung 7,8 MB, glb 2,4 MB gegen Schätzung 3,0 MB); die Rückfrage „Diese Datei gibt es schon …“ beim zweiten Speichern und das Ersetzen; „Netz importieren …“ mit `Körper.stl` und Einheit Zentimeter (Volumen 57 504 787 mm³ = 1000 × 57 504,79 mm³, Abmessungen 900 × 500 × 400 mm), danach Maßstab 1, Position x = 50 und Drehung z = 45° (Abmessungen 90,71 × 69,50 × 40 mm, Volumen 57 504,79 mm³); „Bearbeiten“ bei gewähltem Netz-Körper (Vereinigen, Abziehen, Schneiden, Verrunden und Fase ausgegraut); Projekt anlegen, speichern, Programm hart beenden, neu starten, Sperre übernehmen, laden (gleiche Werte); OBJ-Export des gewählten Netz-Körpers (die ersten Punkte zeigen die Verschiebung und Drehung). **Bei der Handprüfung aufgefallen und in der Korrekturwelle behoben:** (1) Für gewöhnliche Körper mit Kugel-, Zylinder- oder Kegelflächen meldete das Programm „nicht ganz geschlossen“ (Quader plus Kugel: 496 offene Kanten bei „Normal“, 1572 bei „Fein“), und die geschriebenen Dateien hatten tatsächlich offene Kanten. Ursache: Das Vernetzen liefert an Nähten und Polen Rundungsreste von etwa 1e-15 statt genau 0; nach der Umwandlung auf 32 Bit waren die Punkte beider Seiten nicht bitgleich und wurden nicht verschmolzen. Das war kein falscher Alarm. Behoben, indem beim Übernehmen von Körpern aus dem Kernel Koordinaten, die nur um Rundungsreste von 0 abweichen, auf 0 gesetzt werden (`SEAM_ZERO_REL`); Kugel, Zylinder, Kegel und gerundeter Zylinder sind jetzt bei Grob, Normal und Fein geschlossen, auch nach dem Schreiben und Zurücklesen als STL und OBJ. Eingelesene Netze bleiben bewusst exakt (es werden nur bitgleiche Punkte verschmolzen); importierte Netze mit Rundungsresten zeigen deshalb weiter offene Kanten. Das ist eine bekannte Grenze. (2) Das Eigenschaftenfenster zeigte für denselben Netz-Körper „nicht ganz geschlossen“, das Export-Fenster „geschlossen“: Das Eigenschaftenfenster zeigt den Bericht vom Einlesen, das Export-Fenster prüft nach der Lage neu. Behoben durch die Beschriftung „Beim Einlesen“. (3) Nach dem Wechsel des Formats blieb die Zeile „Fertig: … geschrieben“ des vorigen Exports stehen; sie verschwindet jetzt bei Wechsel des Formats oder des Stands. (4) Die Drehung zeigte „-0 °“; winzige Winkel erscheinen jetzt als 0 °. Nicht von Hand geprüft: Abbrechen mitten im Export oder Import, sehr große Netze und die Fenster dabei, OBJ-Import, Dateien aus anderen Programmen, das Öffnen der 3MF- und glTF-Dateien in fremden Programmen. Auch die Korrekturen selbst sind nur durch automatische Tests geprüft, nicht erneut von Hand.

**Was nicht geht oder anders ist**
- **Netz-Körper sind Referenz-Netze.** Vereinigen, Abziehen, Schneiden, Verrunden, Fase und Skizze auf einer Fläche sind für sie gesperrt (ausgegraut, Hinweis „Netz-Körper lassen sich nicht bearbeiten.“). Verschieben und Drehen geht über die eigene Lage im Eigenschaftenfenster, nicht über einen Schritt „Verschieben“ (P10). Ein Bearbeiten-Fenster gibt es für sie nicht.
- **Dateiauswahl:** Das Programm hat ein eigenes Auswahlfenster statt des Dateidialogs von Windows (P14), weil der Systemdialog ein zusätzliches Programmpaket bräuchte. Ein Laufwerk wechselt man nur durch Eintippen des Pfads. Ein Ordner öffnet sich per Doppelklick. Das Lesen eines Ordners geschieht im Hauptfaden; sehr große Ordner und Netzlaufwerke hakeln.
- **Abbrechen:** Wirkt beim Export nur zwischen zwei Körpern; ein einzelner großer Körper rechnet im Hintergrund zu Ende, das Ergebnis wird verworfen (P15). Das Schreiben der Datei lässt sich nicht abbrechen. Die Erfolgsmeldung nennt nur den Dateinamen, nicht den Ordner.
- **Nicht ganz geschlossene Netze** (bekannte Lücken bei Freiformflächen, rund 1 % der Fälle aus Teilprojekt 7 und 8): Das Fenster warnt, die Datei wird trotzdem geschrieben.
- **Grenzen:** 50 Millionen Dreiecke und 8 GiB je Datei, auf Wunsch des Nutzers bewusst groß. Das schützt nicht vor Speichermangel: Das Laden braucht etwa das Dreifache der Textgröße (Text, Kopie, Bytes, Netz); ein einzelnes Netz hat höchstens etwa 2,4 GB Netzdaten, braucht beim Laden aber rund 13 GB Spitze; bei 8 GB Netzdaten im Dokument sind es bis etwa 35 GB.
- **3MF** ohne ZIP64: Einträge und Archiv höchstens knapp 4 GiB, Einträge unter 65 535 je Archiv; darüber benannter Fehler (P7). **glTF** bis 4 GiB, weil die Längen 32 Bit haben (P8). Namen mit den XML-Nichtzeichen U+FFFE und U+FFFF würden die XML ungültig machen (nicht abgefangen).
- **Einheiten:** STL, OBJ und 3MF in Millimeter, glTF in Meter (Millimeter × 0,001). Die Quelleinheit beim Import (Millimeter, Zentimeter, Meter, Zoll) wird als Maßstab des Netz-Körpers gespeichert.
- **Nicht Teil:** STEP-Export (Teil 2 der Austausch-Strecke), Zeichnungen SVG/DXF (Teil 3), Farben, Materialien, Texturen, Import von 3MF, glTF und STEP.

**Einlesen**
- OBJ: Eine Ecke darf nur auf schon gelesene Punkte verweisen (P6). Zeilenfortsetzung mit `\` wird nicht unterstützt. Materialien, Texturen und Gruppen werden ignoriert. Vielecke, die nicht gewölbt nach außen liegen, werden bis höchstens 5000 Ecken zerlegt (`MAX_EARCUT_CORNERS`; die Zerlegung wächst quadratisch, ein Stern mit 100 000 Ecken brauchte 14,7 s im Debug-Build). Sehr fein gerundete Einzel-Vielecke mit mehr als 5000 Ecken gelten wegen der Rundung auf 32 Bit als nicht gewölbt und werden mit Zeilennummer abgelehnt; ein CAD-Export mit einem einzigen riesigen Deckel-Vieleck lässt sich so nicht lesen. Gewölbte Vielecke werden als Fächer zerlegt und sind nur durch die Zeilenlänge begrenzt. Nicht einfache (sich selbst kreuzende) Vielecke, die die Prüfung auf Wölbung bestehen, werden ebenfalls als Fächer zerlegt.
- STL als Text: Ein unbekanntes Wort ist ein Fehler mit Zeilennummer (P5). Eine Binärdatei, die mit „solid“ beginnt und deren Größe nicht passt, wird als Text gelesen; die Meldung ist dann irreführend (so in der Spec gewollt, §3).
- Beide Formate werden beim Lesen verschmolzen: Punkte mit bitgleichen Koordinaten werden zu einem (P3); es wird nicht mit Toleranz verschmolzen.
- Das gelesene Netz ist an den Dokumentstand gebunden: Jede Änderung am Dokument, während das Fenster „Netz importieren“ offen ist, verwirft es, die Datei muss neu gewählt werden (nur Komfort, sicher). Das Namensfeld prüft nicht auf Steuerzeichen oder Länge (wie beim Umbenennen sonst); der Fehlertext nennt die Grenze als „1e9“, nicht in einfacher Sprache; die Fehlermeldung steht im Fenster und in der Statuszeile doppelt.

**Dokument und Anzeige**
- **Base64 im Dokument (Spec §11):** Das Netz steht als Text im JSON; das Dokument wird etwa ein Drittel größer als die reinen Netzdaten und lädt bei großen Netzen langsam. Ein Binärblock wäre ein späteres Teilprojekt. `encode_version` liest jede geschriebene Version zur Kontrolle einmal zurück (doppelte Zeit bei großen Netzen). Nebenbei: Das Dokument ist meist kleiner als eine STL-Datei desselben Netzes (50 Byte je Dreieck gegenüber etwa 4/3 · (12 · Punkte + 12 · Dreiecke)).
- **Dateiformat 3** (P12) wird nur geschrieben, wenn ein Netz-Körper vorkommt; Format 1 und 2 bleiben Byte für Byte gleich (die Golden-Dateien `format1.json`, `format1_voll.json` und `format2.json` sind unverändert, neu ist `format3.json`). Mechanische Testanpassung wie in Teilprojekt 8: Die Tests „zu neu“ in `tests/docstore.rs` und `tests/format2.rs` nehmen jetzt Format 4. Eine Datei mit fehlendem Netz wird als Ganzes abgelehnt.
- **Gelesene Drehung:** serde_json liest Zahlen nicht immer bitgenau zurück (ohne die Option `float_roundtrip`, die hier nicht eingeschaltet ist). Nach Speichern und Laden stimmen Lage und Maßstab genau, Drehung und Volumen bis auf 1e-12 überein.
- **Vorschau ab 5 Millionen Dreiecken:** Die Ansicht zeigt jedes k-te Dreieck, das Netz wird nicht vereinfacht. Volumen, Oberfläche und Export nutzen immer das ganze Netz. Anzeige-Netz und Masseneigenschaften werden bei jeder Änderung am Dokument für alle sichtbaren Netz-Körper neu berechnet, nicht je Version gemerkt; bei Netzen um 5 Millionen Dreiecke ist das spürbar langsam. An der Grenze von genau 5 Millionen ist die Vorschau um ein Dreieck ungenau; schlägt eine Neuberechnung fehl, bleibt der Änderungszähler trotzdem erhöht. Nach dem Wechsel der Netz-Kennung räumt `update_with_report` ungenutzte Netze nicht auf (nur Speicher).
- **Schätzung der Dateigröße:** Das Fenster schätzt aus Dreiecks- und Punktzahl; die Tests verlangen den Faktor 0,5 bis 2 gegenüber der wirklichen Größe. Gemessene Verhältnisse (wirklich zu geschätzt, Kugel und Zylinder bei 0,01 mm Sehne): STL 1,00 (genau); OBJ 0,80 bis 0,90; 3MF 0,81 bis 0,87; glTF 0,81 bis 0,88 (Sehne 0,1, 0,01 und 0,001 mm). Die Schätzung liegt also bei OBJ, 3MF und glTF 10 bis 20 % zu hoch; bei Körpern mit anderem Verhältnis von Punkten zu Dreiecken kann es mehr abweichen.
- Der Export legt die Hilfsdatei unter einem festen Namen (`.Name.planora-tmp`) im Zielordner an; zwei Exporte gleichzeitig in dieselbe Datei sind nicht vorgesehen.

**Abweichung von der Spec:** `earcutr` ist Abhängigkeit von `planora-mesh`, nicht des Kernels (P6). Alle weiteren Abweichungen stehen in Spec §12.

**Laufzeiten und Testreihen**
- Teststand am Ende von Teilprojekt 9: 1311 bestanden, 0 fehlgeschlagen, 29 ignoriert in 177 Testprogrammen (`cargo test --workspace`, Debug, 2026-10-14); Gesamtlaufzeit etwa 8 Minuten mit Übersetzung. Die längsten Testprogramme im Debug-Build brauchen 25,7 s (`nurbs_ssi_stress`), 27,0 s und 27,7 s (alle aus früheren Teilprojekten). Frühere Zwischenstände: 1250 bestanden / 29 ignoriert nach Task 9, 1278 / 29 nach Task 14. `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` und `cargo deny check` (advisories, bans, licenses, sources) sind sauber.
- Lange Reihe: `cargo test -p planora-exchange --release -- --ignored` (`a_long_random_series_keeps_the_volume`: zufällige Körper werden exportiert, wieder gelesen, das Volumen gegen den Kernel verglichen): 1 bestanden in 0,48 s. Andere Pakete haben ihre eigenen langen Reihen (siehe Teilprojekt 7 und 8); sie wurden in diesem Teilprojekt nicht erneut gelaufen, ihre Dateien sind unverändert.
- Alle Testprogramme von `planora-exchange` laufen im Debug-Build unter 1 s (`planora-project`: längstes 5,4 s). Der Formelvergleich nimmt als Sehnenschranke das Zweifache (`SAG_FACTOR = 2.0`): Die Mitte eines Dreiecks im Breiten-/Längenraster liegt bis zu zwei Sehnenhöhen unter der gekrümmten Fläche (gemessen an der Kugel 1,08- bis 1,10-fach).
- Die Anzeigetests der Fenster (`export_dialog` 11, `import_dialog` 8, `exchange_infra` 9 plus Nachträge) laufen ohne echtes Fenster; zwei Mutationsproben (Prüfung auf veraltetes Netz im Export und im Import ausgeschaltet) schlugen wie erwartet fehl.
- Lizenzen: keine neue Abhängigkeit, `cargo deny check` sauber. Die geschützten Pfade (Boolesche Operationen und vier Dateien von `planora-mesh`) und die Golden-Dateien 1 und 2 sind bitgleich zu Beginn des Teilprojekts.

## Teilprojekt 10 – STEP-Export
Stand: 2026-10-14. Körper lassen sich als STEP-Datei ausgeben („Datei“ → „Exportieren …“, Format „STEP – für andere CAD-Programme (exakt)“). Nur Schreiben, keine Farben, Materialien oder Baugruppen, ein Teil je Körper, Einheit Millimeter. Spec: `specs/2026-10-14-step-export-design.md` (Abschnitt 9 listet, was bei der Umsetzung anders wurde). Plan: `plans/2026-10-14-step-export.md`.

**Handtest (2026-10-14, Release-Build, Fusion 360, vom Nutzer selbst ausgeführt).** Der Nutzer öffnete die elf Beispieldateien aus `D:\Testumgebung\step-proben` in Fusion 360 („Datei → Öffnen → Von meinem Computer“). Alle elf (Alle, Rundung, Ecke, Fase, Variabel, Kreuz, Kuppel, Zylinder, Kegel, Kugel, Hohl) wurden ohne Fehlermeldung geöffnet. Verglichen wurden Volumen und Fläche bei zwei Dateien: **Kugel** 1436,755 mm³, Oberfläche 615,752 mm², Schwerpunkt 0/0/0, das sind genau die Werte der exakten Kugel mit Radius 7 (4/3·π·7³ und 4·π·7²); der Sollwert 1436,072 in der Tabelle unten ist das Volumen des einbeschriebenen Prüfnetzes, nicht das der exakten Kugel. **Hohl** 936,00 mm³, Oberfläche 696,00 mm², Schwerpunkt 5/5/5, das passt zum erwarteten Körper (1000 − 64; 600 + 96), der Hohlraum wurde als Hohlraum erkannt. Beide erscheinen in Fusion unter „Körper“ als Volumenkörper (mit Masse und Material), Einheit mm. Nicht verglichen: Volumen und Flächenzahl der übrigen neun Dateien, Fusion-Version, und der Export über das Fenster der App (die Dateien stammen aus dem Testprogramm, nicht aus einem Lauf im Fenster). Sollwerte der Beispieldateien: Beispieldateien erzeugt `cargo test -p planora-step --release -- --ignored write_samples_for_the_hand_test` (Ordner über die Umgebungsvariable `PLANORA_STEP_SAMPLES`): Rundung (7 Flächen, 991,395 mm³), Ecke (10; 975,530), Fase (7; 988,750), Variabel (7; 993,022), Kreuz (7; 3717,342), Kuppel (7; 1133,933), Zylinder (3; 942,229), Kegel (2; 339,217), Kugel (1; 1436,072), Hohl (12; 936,000) und `Alle.step` mit 10 Teilen. Was dort auffällt, wird eine Folge-Aufgabe, keine Nacharbeit unter Zeitdruck.

**Fenster schließen während STEP geschrieben wird (behoben in der Korrekturwelle).** Befund der Abschlussprüfung: „Schließen“, das X oder der Wechsel in den Skizzenmodus verwarfen das Fenster, der Arbeitsthread brach ab, die Hilfsdatei wurde gelöscht und es gab keine Meldung; beim „Ersetzen“ blieb so die alte Datei liegen, ohne dass der Nutzer es merkte. Jetzt sind „Schließen“ und das X des Fensters sowie der Wechsel in den Skizzenmodus beim Schreiben einer STEP-Datei wirkungslos (das Fenster bleibt offen); im Fenster steht „Das Schreiben läuft noch. Bitte warten oder erst abbrechen.“. „Abbrechen“ bleibt möglich und meldet „Abgebrochen“, die alte Datei bleibt unverändert. Betrifft nur STEP; die Netzformate schreiben auch ohne Fenster zu Ende. **Grenze:** Beendet man das Programm oder schließt man den Tab, während eine STEP-Datei geschrieben wird, wird das Schreiben abgebrochen und die alte Datei bleibt unverändert (ohne Meldung). Tab-Schließen und Beenden sind absichtlich nicht gesperrt, damit das Programm nie unbeendbar wird.

**Was nicht geht oder anders ist**
- **Prüfung ohne fremdes Programm:** Die Dateien werden beim Bauen nur mit einem eigenen Leser geprüft (Aufbau, alle Verweise, Schreibweise), mit einem Modell, das Ebene, Zylinder, Kegel, Kugel, Torus und B-Splines mit eigenen Formeln auswertet, mit Topologie- und Euler-Prüfung, mit dem Vergleich gegen den Kernel (Punkte und Außennormalen) und mit einem Netz aus den STEP-Flächen (Volumen und Abstand). Das ersetzt die Gegenprobe in einem fremden Programm nicht; fremde Programme können strenger sein.
- **Pole und Kegelspitzen** (Kugel, Kegel, Freiform-Pole) sind ein Eckpunkt, die Naht läuft in der Randschleife hin und zurück, die Fläche bleibt ganz (Spec §3). Mehrere Programme schreiben Kugeln so. Nimmt Fusion 360 das nicht an, wird die Kugel an einem zweiten Meridian (u = π) in zwei Flächen geteilt; das wäre eine eigene Folge-Aufgabe. (Die in der Spec zuerst genannte Ausweichlösung „Pol als `VERTEX_LOOP`“ war falsch, siehe Spec §9.)
- **Spindelringe lassen sich nicht ausgeben:** Dreht man in der Skizze einen Bogen, der die Achse berührt oder überschreitet, entsteht ein Ring mit kleinem Radius ≥ großem Radius. Dafür gibt es in STEP nur den Sonderfall `DEGENERATE_TOROIDAL_SURFACE`, den das Programm nicht ohne Gegenprobe schreiben will. Der Körper wird mit Grund ausgelassen. Folge-Aufgabe, sobald es eine Gegenprobe gibt.
- **Prüfwerkzeug (nur Tests):** Bei Körpern mit Freiform gilt die Schranke der Datei (1e-5 mm, P10) auch für ihre Geraden und Kreise; vorher schlug ein gedrehter Schnitt aus Quader und NURBS-Zylinder an einem Eckpunkt 6e-9 neben einer Geraden an. Dreiecke aus drei fast auf einer Linie liegenden Punkten zählen beim Ausrichten nicht mehr (Fläche unter 1e-9). Ein um 1e-3 verschobener Eckpunkt wird weiterhin gefunden. `check_body` nimmt die Toleranz jetzt aus den `StepOptions`.
- **Doppelte Rechenzeit:** Jeder Körper wird vor dem Schreiben einmal probeweise geschrieben (P3), das Fenster prüft dazu schon beim Öffnen (`prepare_step`), also dreimal insgesamt. Bei sehr großen Freiformkörpern spürbar. Körper mit mehreren Schalen werden zusätzlich zweimal über ein grobes Netz sortiert.
- **Hohlräume über ein grobes Netz** (`CLASSIFY_CHORD = 0,05 mm`, P9): Hohlraum und äußere Schale werden am Vorzeichen des Netzvolumens unterschieden, ein Hohlraum gehört zur kleinsten äußeren Schale um seinen ersten Eckpunkt. Scheitert das Netz, wird der Körper benannt ausgelassen. Bei verschachtelten Hohlräumen (Körper ⊃ Hohlraum ⊃ Insel ⊃ Hohlraum) kann ein Hohlraum, dessen erster Eckpunkt näher als die Sehnenabweichung (0,05 mm) an einer gekrümmten Fläche der Insel liegt, der falschen Hülle zugeordnet werden. Das ist sehr unwahrscheinlich, der Kernel erzeugt solche Körper kaum; beobachtet wurde es nicht.
- **Unsicherheit 1e-5 mm** bei Körpern mit Freiform (P10), sonst 1e-6 mm; fremde Programme heilen Kanten bis zu dieser Größe.
- **Kegel:** Nur die Mitte des Flächenbereichs wird auf die Spitze geprüft; der Kernel erzeugt keine Fläche, die über die Spitze reicht.
- **Freiformkanten über die Naht:** Kanten geschlossener Freiformkurven (der Kernel faltet `t` über das Bereichsende) werden über ihre Eckpunkte begrenzt. Ob fremde Programme bei offenen, nur als geschlossen gekennzeichneten B-Splines über das Ende laufen, ist ungeprüft.
- **Abbrechen:** Beim Prüfen zwischen den Körpern und alle 4096 Einträge, beim Schreiben alle 4096 Einträge.
- **Namen:** Leere Namen werden „Körper n“, Namen über 256 Zeichen gekürzt (an einer Zeichengrenze); das Fenster nennt gekürzte Namen nach dem Schreiben („Name gekürzt …“) und kürzt lange Namen in seinen Listen auf 40 Zeichen. Zwei Körper mit demselben Namen werden zwei Teile mit demselben Namen (nicht umbenannt).
- **Hilfsdatei** mit festem Namen (`.Name.planora-tmp`): zwei gleichzeitige Exporte in dasselbe Ziel würden kollidieren; das Fenster startet nur einen.
- **Fenster:** `ExportFormat` statt `Format` (P14) samt einer mechanisch angepassten Testzeile. Nachbesserungen vor dem Zusammenführen (alle mit Test und Mutationsprobe): Der Abbrechen-Knopf hängt am laufenden Schreibweg; die Meldung „Fertig“ gehört zu dem Format, mit dem geschrieben wurde, und verschwindet beim Formatwechsel; „Abbrechen“ und „Neu berechnen“ betreffen nur das gewählte Format; „x.stp“ wird zu „x.step“ (mit Rückfrage beim Ersetzen); gekürzte Namen werden gemeldet; der Hinweis am gesperrten Knopf nennt bei 0 Körpern den Grund. Offen bleibt: `.stp`-Dateien stehen nicht in der Dateiliste (die Auswahl kennt nur eine Endung).
- **Nicht Teil:** Einlesen von STEP, Farben, Materialien, Baugruppen, Zeichnungen (SVG/DXF).

**Kernel-Grenze, die beim Testen auffiel:** Schräge Zylinder mit Zylindern oder Kugeln rechnet der Kernel noch nicht. Die Zufallsreihe deckt deshalb nur Quader mit Quader, Zylinder oder Kugel ab; von 300 Fällen werden etwa 20 wegen leerer Schnittmenge übersprungen.

**Laufzeiten und Testreihen**
- Teststand am Ende von Teilprojekt 10: 1404 bestanden, 0 fehlgeschlagen, 31 ignoriert in 193 Testprogrammen, Gesamtlaufzeit etwa 9,6 Minuten mit Übersetzung.
- Lange Reihe: `cargo test -p planora-step --release -- --ignored`: 2 bestanden in 3,7 s (die 300-Fälle-Reihe und das Schreiben der Beispieldateien). Die Reihe prüft 300 Zufallskörper (Startwert fest) gegen den Kernel; sie fand einen Fehler im Prüfwerkzeug (sehr schlanke Ellipse, siehe Spec §9), nicht im Schreiber.
- Größtes Debug-Testprogramm von `planora-step`: `freeform` etwa 12 s, `stage3` etwa 5 s, alle anderen unter 1 s.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` und `cargo deny check` sind sauber. Keine neue externe Abhängigkeit. Kernel, `planora-math`, `planora-mesh`, `planora-exchange`, `planora-feature`, `planora-project`, `planora-render`, `planora-sketch` und die Golden-Dateien sind bitgleich zu `07bac2b`.

## Teilprojekt 11 – Zeichnungen (Ansichten)
Stand: 2026-10-15. Aus den exakten Körpern eines Dokuments entstehen technische Zeichnungen: sieben Ansichten (Vorne, Hinten, Links, Rechts, Oben, Unten, Isometrisch) mit sichtbaren und verdeckten Kanten, auf einem Blatt von A4 bis A0 mit Maßstab, ausgegeben als SVG und DXF („Datei“ → „Exportieren …“). Die Zeichnung gehört zum Dokument (Dateiformat 4, nur wenn es eine Zeichnung gibt), wird im Menü „Zeichnung“ angelegt und rechnet sich nach Änderungen am Modell neu. Das ist Teil 3a von dreien; Maße und Schriftfeld (3b) und Schnitte (3c) folgen. Spec: `specs/2026-10-14-zeichnungen-ansichten-design.md` (der Abschnitt „Festlegungen der Umsetzung“ am Ende listet, was bei der Umsetzung anders wurde). Plan: `plans/2026-10-14-zeichnungen-ansichten.md`.

**Entscheidungen mit dem Nutzer (2026-10-14):** Zweck Fertigung, Dokumentation und CNC/Laser, aber jetzt nur die Ansichten (Z1); echte Kurven statt Streckenzügen (Z2); Sichtbarkeit exakt gegen die echten Flächen, nicht am Netz (Z3); Netz-Körper werden nicht gezeichnet (Z4); Zeichnung ist Teil des Dokuments und live verknüpft (Z5); eigener Arbeitsbereich im Dokument-Tab (Z6); sieben Ansichten, frei platzierbar, je Ansicht ein Maßstab (Z7); Erstwinkel und Dritter Winkel wählbar (Z8); keine neue externe Abhängigkeit (Z9).

**Was geliefert wurde**
- Neues Paket `planora-drawing` (Berechnung ohne Oberfläche): Ansichtsbasis und Blattlayout, Kanten und Umrisse (Zylinder, Kegel, Kugel exakt; Ring und Freiform als angenäherter Spline, 1e‑4 mm), Teilen an allen Kreuzungen, Sichtprüfung mit Strahlen gegen die echten Flächen, Zusammenfassen, Blattmodell, SVG- und DXF-Schreiber, Schreiben über eine Hilfsdatei.
- `planora-feature`: Zeichnungen im Dokument (anlegen, ändern, löschen, geprüft); `planora-project`: Dateiformat 4; `planora-app`: Zeichnungsbereich, Browser-Abschnitt, Fenster „Neue Zeichnung“ und „Ansicht hinzufügen“, Berechnung im Hintergrund, Export als sechstes und siebtes Format im Export-Fenster.
- Alle Teile, die der Nutzer sieht, haben deutsche Texte in Laiensprache; Zeichnungen lassen sich auch in alten, schreibgeschützten Versionen ansehen (nicht ändern).

**Handtest: SVG in Ordnung; DXF-Neutest in Fusion 360 für vier Proben bestanden (2026-10-15).** Fusion 360 lehnte die erste DXF-Fassung ab („DXF-Datei konnte nicht eingefügt werden“). Ursache (mit ezdxf-Varianten eingegrenzt): den Ebenen fehlte der Verweis (Gruppe 390) auf den Plotstil-Platzhalter samt Klassen und Objektkette. Behoben: jede Ebene trägt 390, `CLASSES` enthält `ACDBDICTIONARYWDFLT` und `ACDBPLACEHOLDER`, `OBJECTS` die Kette `ACAD_PLOTSTYLENAME` → Vorgabeverzeichnis → Platzhalter; ein unabhängiges Python-Prüfwerkzeug (ezdxf, nur außerhalb des Repos) liest die neuen Proben ohne Fehler. Zweiter Fund: Fusion 360 zeichnete eine volle Ellipse nicht (nur der Mittelpunkt erschien), wenn ihr Endparameter als auf neun Stellen gerundetes 2π (`6.283185307`, knapp unter 2π) geschrieben war; mit genau 2π oder in zwei Hälften wird sie gezeichnet. Behoben: Anfangs- und Endparameter der Ellipse stehen in voller `f64`-Genauigkeit, eine volle Ellipse läuft von `0` bis genau `6.283185307179586`, ein Ende bei 2π bleibt 2π (nicht 0); Bögen mit höchstens einem Milliardstel Grad unter 360° werden zum Kreis. Der DXF-Import ist damit in zwei Schritten korrigiert (Plotstil-Kette, genaues 2π); SVG war immer in Ordnung. Die Proben sind neu geschrieben; die Anzeige der neuen DXF-Dateien in Fusion 360 steht wieder aus. Hier trägt der Controller nach der Rückfrage beim Nutzer ein, was beim Öffnen der Proben in Fusion 360 (DXF als Skizze einfügen) und im Browser oder in Inkscape (SVG) herauskam: ob die Geometrie ankommt, ob die Namen der Blöcke stimmen, ob die verdeckten Linien gestrichelt sind, ob Längen und Kreisdurchmesser stimmen. Die Proben erzeugt `cargo test --release -p planora-drawing --test hand_samples -- --ignored` (Ordner `D:\Testumgebung\zeichnung-proben`, abweichend über `PLANORA_SAMPLES_DIR`): 13 Körper (Quader, Zylinder, Kugel, Kegelstumpf, Bohrung_X, Bohrung_Y, Winkel, Tasche, Durchbruch, Rundung, Torus, Freiform, Dritter_Winkel) je als SVG und DXF auf A3 quer mit Vorne, Oben, Links und Isometrisch, dazu `ERWARTUNG.txt` mit den erwarteten Gesamtlängen (sichtbar/verdeckt je Ansicht, aus Formeln gerechnet, beim Schreiben gegen die Bibliothek geprüft; bei Quader, Zylinder, Kugel, Kegelstumpf, Bohrungen, Winkel, Tasche, Rundung, Torus von oben stimmen alle Formelwerte) und der Liste dessen, was man ansehen soll. Fusion-Punkte aus der DXF-Prüfliste: kommt die Geometrie aus den Blöcken (`INSERT`) in der Skizze an, sonst Geometrie direkt in `ENTITIES` schreiben; Blocknamen mit Umlaut (`\U+XXXX`) oder ASCII; Ebenen `SICHTBAR`/`VERDECKT`, Linienart `HIDDEN` (3 mm Strich, 1 mm Lücke); Einheit Millimeter, Blatt 420 × 297, Rahmen 10 mm; Ellipsen, Bögen über 0° und Splines an der richtigen Stelle, keine Warnung. Warnt Fusion, ist der Ausweg, in `OBJECTS` Layouts (`ACAD_LAYOUT`) und an den Blockeinträgen die Verweise (340) zu ergänzen.

**Handtest Fusion 360 (vom Nutzer ausgeführt):** SVG gut; DXF nach zwei Korrekturen (Zeichenstil-Kette mit 390 an den Ebenen samt `ACDBDICTIONARYWDFLT`/`ACDBPLACEHOLDER`, genau 2π bei vollen Ellipsen) gut für Linien, Kreise, Bögen und Ellipsen. Geschlossene Splines (erster Kontrollpunkt gleich letztem, etwa ein angepasster Freiform-Umriss als Ring) werden dargestellt, aber nicht als Fläche (Profil) erkannt: Fusion meldet „nicht geschlossen“. Probiert wurden das Schließen-Kennzeichen der Datei (Variante G3) und zwei offene Hälften, die an beiden Enden zusammenstoßen (G4), beides ohne Erfolg; dieselbe Zeichnung ohne den Spline (G1) ergibt ein sauberes geschlossenes Kreisprofil. Der Fall, an dem es auffiel (Freiform von oben, Umriss-Spline auf dem Kreis r = 8), tritt seit Commit 15d4dc7 nicht mehr auf (angepasster Umriss wird mit dem genauen Kreis zusammengefasst), die Proben sind also nicht betroffen; andere geschlossene Freiformringe (etwa geschlossene Freiformkanten) können es weiter zeigen. Mögliche Abhilfen (nicht umgesetzt): viele kurze Bézier-Stücke oder geschlossene Freiformkurven als Streckenzug ausgeben. **Neutest bestanden (2026-10-15):** Die neu erzeugten DXF-Proben Freiform, Zylinder, Torus und Rundung lassen sich in Fusion 360 richtig einfügen; der SVG-Test ist erledigt. Geschlossene Splines werden weiter nicht als Fläche erkannt (bekannt, siehe oben). **Nicht einzeln neu geprüft** nach den letzten Korrekturen: Quader, Kugel, Kegelstumpf, Bohrung_X, Bohrung_Y, Winkel, Tasche, Durchbruch, Dritter_Winkel. Inkscape und andere Programme sind nicht geprüft.

**Was nicht geht oder anders ist**

*Umrisse und Sichtbarkeit*
- **Stille Lücke bei Freiform-Umrissen:** Umrissschleifen, die kleiner sind als etwa eine halbe Zelle des feinsten Rasters und an keiner Kantenmitte und Zellmitte das Vorzeichen wechseln, bleiben unentdeckt; ebenso Kanten, die dreifach gekreuzt werden. Der Umriss fehlt dann ohne Meldung (Aufgabe 7, Grenze a). Das ist die gefährlichste Grenze, weil nichts darauf hinweist.
- **Umkehrstellen planarer Splines:** Ein Freiform- oder Spline-Umriss, der im Bild umkehrt (eine ebene Kurve, die man genau von der Kante sieht, etwa ein ausgezogenes Spline-Profil), wird an der Umkehrstelle nicht geteilt und kann doppelt über sich liegen (Aufgabe 8, Grenze 1). Abhilfe: die Umkehrstellen (`tip_params`) auch für ebene Striche berechnen, deren Ebene parallel zur Blickrichtung liegt.
- **Freiform mit geschlossenem Pol** wird in Seitenansichten mit „nicht unterstützt“ abgelehnt (Grenze b). Eine Ansicht knapp neben der Ebene eines Rings (schräg liegender Ring) bricht mit dem Fehler „Umriss zu stark gekrümmt“ ab (Grenze c).
- **Prüfung der Freiformen:** Tangentiale Kuppeln und Kuppeln mit Überhang sind nur mit einem eigenen Richter und einer Abstandsprüfung geprüft, nicht mit dem Referenznetz (das kann tangentiale Übergänge und Überhänge nicht beurteilen). Horn- und Spindelringe gibt es nur als Rauchtest (Aufgabe 8, Grenze 2).
- **Prüfung (f) und Überdeckungen:** Die Prüfung auf Doppellinien sieht keine „verdeckt auf verdeckt“ und keine Überdeckung unter etwa 9 % eines Stücks. Die Überdeckung wird an Stichproben entschieden: beide Enden des Teilbereichs, ¼, ½, ¾ und die Stellen des Goldenen Schnitts (N1, Abschlussprüfung; eine Welle, die eine Gerade genau an den Vierteln kreuzt, bleibt erhalten). Eine Kurve, die die andere genau an allen sieben Stellen berührt und dazwischen abweicht, könnte weiterhin weggelassen werden; in `compute_view` ist sie dort aber schon an ihren Kreuzungen geteilt.
- **Zuschnitt der Umrisse auf die Fläche (Abschlussprüfung, I1 – behoben):** Umrisse werden an 256 gleichmäßigen Stellen und zusätzlich an Stellen aus dem Flächenrand geprüft: Randpunkte (Geraden 16, Kegelschnitte alle 5°, Freiformkanten 64 Stellen) auf den Umriss abgebildet, und je Randstück jedes örtliche Abstandsminimum zum Umriss mit dem Goldenen Schnitt verfeinert – die **genaue** Kreuzung oder Berührung wird Abtaststelle, dazu die Mitten zwischen benachbarten Stellen. So werden schmale Kerben, kleine Bohrungen, schmale Flächenstreifen und auch schmale **schräg** liegende Schlitze erkannt (Regressionstest: die Schlitz-Probe des Prüfers mit β von 0,25° bis 5° an Zylindern r = 20 und r = 50, auf der −x-Seite und auf der Naht, und an einer Kugel; ein schräger Schlitz in einen Kegel lässt sich mit dem Kernel nicht schneiden). Obergrenze 65 536 Stellen je Umrissstück, darüber „zu aufwendig“ (mit Test). **Verbleibende Grenzen:** Ein Randstück, das den Umriss zwischen zwei seiner Stützstellen zweimal kreuzt (eine Freiformkante mit einer Welle kürzer als ihr Stützstellenabstand, 64 Stellen je Kante), liefert nur eine der beiden Kreuzungen. Ebenso ein Kreis- oder Ellipsenrand, der den Umriss nur knapp anschneidet (Abflachung oder Bohrung an einer Kugel): Liegen beide Kreuzungen innerhalb eines 5°-Schritts des Rands (Lücke unter etwa 0,09·Randradius) oder ist der angeschnittene Streifen flacher als die Genauigkeit des Randpolygons in `point_in_face` (etwa 0,001·Randradius), läuft der Umriss ohne Meldung durch (gemessen: Kugel R = 100, Abflachung mit Randradius 50, Lücke bis 4 mm). Die Linie liegt in der Ansicht dann bis auf Bruchteile eines Mikrometers auf der Randkante; es entsteht eine Doppellinie, keine sichtbar falsche Linie. `point_in_face` baut die Polygone je Aufruf neu (Kosten).
- **Verdeckte Kanten:** Neue Normansichten zeigen sie von Anfang an (Werkstattzeichnung), die isometrische Ansicht nicht; je Ansicht umschaltbar.
- **Übergänge der Sichtbarkeit:** Stücke kürzer als `OWN_EPS` (1e‑5 mal Ausdehnung) ohne brauchbaren Strahl erben das Urteil des Nachbarn; ein Abschnitt dieser Länge könnte falsch zählen (unter der Zeichenauflösung). Der Umriss einer Freiform wird am wahren Umrisspunkt beurteilt; zwei Umrisszweige, die näher als 1e‑4 mm beieinander liegen, können verwechselt werden.
- **Kleine Reste:** Nach dem Zusammenfassen können Reste bis zu vier Fangabständen übrig bleiben, während die Prüfung nur 1e3 mal die geometrische Toleranz zulässt (N3, Angleichen oder dokumentieren).
- **Kernel-Grenze:** Der Kernel schneidet zwei sich kreuzende Zylinder nicht (`UnsupportedIntersection`). Die Prüfkörper haben deshalb statt der runden Querbohrung einen um 45° gedrehten Vierkant-Durchbruch; echte Zylinder-Zylinder-Schnittkanten kommen nur über Freiformkanten vor.

*Grenzen der Arbeit*
- `MAX_STROKES = 20 000` zählt nach dem Verwerfen der in der Ansicht zu Punkten entarteten Striche; die Rohgrenze ist das Vierfache. 2000 Kästen: Vorne geht, Isometrisch meldet „zu aufwendig“ (24 000). Weitere Grenzen: 200 000 Stücke, 50 Millionen Paartests, 20 Millionen Sichtprüfungen, 400 000 Umrisspunkte je Ring- oder Freiformfläche (Letztere ist im Plan nicht vorgesehen und kam in Aufgabe 7 dazu).
- **Leistung:** Ein Ring (R = 10, r = 3) braucht je Ansicht im Release-Build etwa 30 ms (vorne), 35 ms (oben) und 45 ms (isometrisch), gemessen 2026-10-15 mit `compute_view`. Deckungsgleiche Kegelschnitte (eine Platte mit vielen Löchern von oben) werden vorab als gleiches Bild erkannt; die Teilstellensuche kostet dort kaum Paartests. Die Umrisse einer Platte mit 1000 Bohrungen kosten von vorne etwa 3 s, isometrisch etwa 5 s (Release; vor allem `point_in_face`, das die Randpolygone je Aufruf neu baut). `trim_covered` und `Images::point` bauen bei freiformlastigen Szenen Hüllboxen und Auswerter immer neu. `Scene::new` ist teuer (Messung je Randstück).
- **Zeichnen im Fenster:** Kurven werden je Frame abgeflacht; beim Ziehen einer Ansicht entsteht je Frame ein neues Blattmodell, also werden alle Kurven je Frame neu abgeflacht (der Cache hängt am Blattmodell und der Zoomstufe). Bei vielen großen Kreisen kann das Ziehen zäh werden.
- **Ziehen schreibt je Zeigerbewegung eine Änderung** (`edit_count` wächst um 1 je Frame, kein Neuberechnen). Zusammenfassen beim Loslassen wäre sauberer.

*DXF und SVG*
- DXF (Version R2000, Text) mit 9 Nachkommastellen; Koordinaten, Verhältnis und Gewichte sind auf 9 Stellen gerundet, die Parameter der Ellipse stehen in voller Genauigkeit; Ellipsen sind damit auf etwa 1,5e‑9 mm + 5e‑10 mal Halbachse genau, rationale Splines auf etwa 1e‑8 mm, Knoten bei beliebigen Knotenvektoren bis etwa 5e‑10 mal Schenkellänge je Knotenspanne. Bögen über 0° haben Anfang größer als Ende (richtig nach DXF).
- Doppelte Linie bei angepassten Umrissen (Probe Freiform, Ansicht Oben; auch gerundeter Zylinder von oben): behoben. Der Umriss einer Freiform- oder Torusfläche ist nur auf `SILHOUETTE_TOL` = 1e‑4 mm angenähert und lag im Bild auf einer genauen Kante (Kreis), ohne mit ihr zusammengefasst zu werden (Deckungstoleranz der genauen Paare rund 1e‑6 mm). Jetzt gilt für Paare mit einem angepassten Umriss die Toleranz `SILHOUETTE_TOL + Gleichheitsabstand` (zwei angepasste: `2·SILHOUETTE_TOL + …`); die genaue Kurve bleibt, der angepasste Spline entfällt (bei gleicher Sichtbarkeit; sichtbar gewinnt). Die Toleranz der genauen Paare blieb unverändert. Grenze: liegt ein angepasster Umriss sichtbar genau auf einer verdeckten genauen Kante, bleibt der sichtbare Spline (eine Linie, richtig sichtbar), nicht die genaue Kante.
- Es gibt einen leeren `CLASSES`- und einen kleinen `OBJECTS`-Abschnitt (Wurzel-Wörterbuch und `ACAD_GROUP`), aber **keine Layouts und keine 340-Verweise** an den Blockeinträgen. Falls der Handtest in Fusion scheitert, ist das der erste Ausweichschritt. Blocknamen: Komma ersetzt, Eindeutigkeit ohne Groß-/Kleinschreibung, Nicht-ASCII als `\U+XXXX`. Wie Fusion die Geometrie in `INSERT`-Blöcken behandelt, ist ungeprüft. Es gibt ein minimales `DIMSTYLE`, keine Verweise (390) an den Ebenen.
- SVG: Bögen als Stücke bis 90° statt Halbbögen (die Rundung auf sechs Stellen verschöbe sonst den Mittelpunkt bis etwa 3e‑3 mm); Splines werden auf 2e‑7 mm genau in kubische Béziers zerlegt, rationale Kegelschnitte als 8 Kurvenstücke je Viertelkreis (kein `A`). Gegen den Uhrzeigersinn laufende Bögen haben im gespiegelten Blattsystem das Kennzeichen `sweep = 1` (der Plan nannte fälschlich 0; von Hand gerechnet, mit eigenem Leser und im Browser nachgewiesen). Die `id` einer Ansichtsgruppe ist der Name; Namen mit Leerzeichen sind keine strengen XML-Kennungen; die Eindeutigkeit der Namen wird vor dem Bereinigen geprüft statt danach.
- Dateinamen: Eine fremde Endung wird angehängt statt ersetzt (`plan.dxf` als SVG gibt `plan.dxf.svg`); reservierte Windows-Namen („CON“), ein Punkt am Ende und sehr lange Namen im Vorschlag werden nicht behandelt (schon vorher so).

*Dokument, Datei und Oberfläche*
- Der Serde-Schalter `float_roundtrip` ist im ganzen Arbeitsbereich eingeschaltet (für alle Pakete, keine neue Abhängigkeit): Zahlen überleben Speichern und Laden bitgenau. Kosten: Zahlen werden etwas langsamer gelesen; alte Dateien lesen das letzte Bit gegebenenfalls anders (nur genauer).
- Listenlängen werden beim Lesen begrenzt (`MAX_DRAWINGS`, `MAX_VIEWS`), Kennung 0 gilt als beschädigt. Der Zähler der nächsten Zeichnung wird nur mit Zeichnungen gespeichert; löscht man alle, speichert und lädt, beginnt er wieder bei 1 (die Kennungen gelten nur innerhalb eines Dokuments). Die nachträglichen Prüfungen (`check_len`, `MAX_*`) sind nach dem begrenzten Lesen doppelt.
- `check_head` in `docstore.rs` (Zeile 180) vergleicht die Rahmen-Dateien mit `FORMAT_VERSION` statt mit `CONTAINER_FORMAT` (schon vor diesem Teilprojekt so, wirkt sich erst bei einem Format über dem Rahmenformat aus). Der Fehlertext nennt den Zeichnungsnamen ungekürzt. Die Testdaten duplizieren die Golden-Dateien unter `tests/data`.
- Die Arbeitsfäden der Zeichnungs- und Exportjobs heißen „planora-austausch“ (kosmetisch). Der Zeichnungsbereich und der Export rechnen die Ansichten getrennt (je eigener Job, F11); die Rechnung wird also einmal im Bereich und einmal beim Export gemacht.
- **Export:** `build_sheet` lässt Ansichten, die sich nicht auslegen lassen, weg. Beim Export kommt das praktisch nicht vor (das Dokument prüft Lage, Maßstab und Eltern; scheitert eine Ansicht, bricht der Export ohnehin ab); fehlt trotzdem eine, meldet das Export-Fenster jetzt „Die Zeichnung enthält Ansichten, die sich nicht auf dem Blatt anordnen lassen.“ statt weniger Ansichten zu schreiben. Eine Panik im Rechenjob ergibt eine allgemeine Meldung statt „Die Ansicht …“; bei mehrfach vorkommender Art steht nur die erste Ansicht im Fehlertext. Der Hinweis zu Netz-Körpern weicht im Wortlaut von der Zeichnungsanzeige ab. Ein Test (Verdeckte Kanten aus, mit Bohrung) fehlt.
- **Browser und Bereich:** Die Auswahl im Browser bleibt bei einer 3D-Auswahl markiert (`selected_drawing`); `forget()` setzt `last_drawing` nicht zurück; Umbenennen mit Eingabetaste im selben Frame; ein direkter Wechsel des Bereichs ohne `open()` lässt Auswahl und Zoom einen Frame lang stehen; schlägt das Anlegen fehl, erhöhen Hinzufügen und Löschen die Änderungszahl um zwei; Fehlertexte in sehr kleinen Rahmen sind abgeschnitten. `drawing_view.rs` hat über 1100 Zeilen (Geometriehilfen auslagern).
- Aufgabe 12: Der Test für einen reinen Elternzyklus isoliert den Fall nicht; ein doppelter Name wird erst nach der Prüfung gemeldet. Aufgabe 9: Verweise in den Kommentaren auf `piece_to_out` sind veraltet; `Out2d::point` baut den Spline-Arbeitsraum je Aufruf; Stücke mit `t0 > t1` werden aufsteigend ausgegeben; es gibt keinen Durchlauf-Test mit einer geschlossenen Freiformkante über `compute_view`.
- Aufgabe 2 bis 6 (Kleinkram): Kein Test für einen Knick knapp über oder unter 0,5°; `NonFinite` nur für `t0` und `t1`; die Kippschleife in `bbox2` hat bei absurden Parametern keine Obergrenze (`periodic_in_range` ist mit `MAX_LAPS = 8` begrenzt); Teilbereichs-Hüllboxen in `isect.rs` stammen nur aus Stichproben (strikt wäre die Hülle der Kontrollpunkte je Knotenspanne); die Blattprüfung der Unterteilung greift bei Breite 0 nie; knapp streifende Strahlen an Kegelschnitt-Flächen liefern zwei fast gleiche Treffer statt „unsicher“ (die Kippabstimmung entscheidet, es zählt nicht als Verdeckung); ein Test dauert im Debug-Build etwa 8,5 s (Stufe 5), 28 s (Stufe 6), 166 s (Stufe 7).

**Wie geprüft wurde:** Alle Rechenstufen sind mit eigenen Formelmodellen, einem eigenen Netzprüfer (Referenznetz, Strahlen gegen das Netz), einem unabhängigen Richter für Freiformen, eigenen Lesern für SVG und DXF und einer Zufallsreihe geprüft; für jeden Regressionstest gab es einen Mutationsbeweis (im jeweiligen Aufgabenbericht).

**Folge-Aufgaben nach Priorität**
- *Hoch*
  1. Stille Lücke bei winzigen Umrissschleifen von Freiformen schließen (feineres Raster oder Nachsuche an Rändern; zumindest eine Meldung, wenn das Raster nicht ausreicht).
  2. Umkehrstellen planarer Splines teilen (doppelte Linien bei ausgezogenen Spline-Profilen).
  3. Handtest in Fusion 360 und Inkscape, danach gegebenenfalls DXF-Layouts (`ACAD_LAYOUT`, 340) oder Geometrie direkt in `ENTITIES`.
  4. Freiform-Umriss: Rasterzellen, in denen eine Ecke keine Normale hat (Pol), werden übersprungen, wenn die übrigen Ecken gleiches Vorzeichen haben (`outline_chains` in `silhouette.rs`); ein Umrissstück, das nur in einer solchen Zelle liegt, fehlt ohne Meldung. Nachsuche in diesen Zellen oder Meldung.
  5. (erledigt in der Abschlussprüfung: Export nennt nicht anordenbare Ansichten; N1 Enden und mehr Stichproben in `trim_covered`; N2 Frist im Abbruchtest.)
- *Mittel*
  5a. Knapp angeschnittene Kreis- oder Ellipsenränder an Umrissen (Doppellinie, siehe I1): die zwei Kreuzungen innerhalb eines Schritts durch Verfeinern der Randprobe bei kleinem Abstand zum Umriss finden. Test: Kugel R = 100 mit Abflachung, Randradius 50, Lücke 0,5 bis 4 mm.
  6. Leistung: `point_in_face` mit vorbereiteten Randpolygonen für analytische Flächen (Platte mit 1000 Bohrungen: Umrisse 3 bis 5 s), `trim_covered`, `Scene::new`, Zeichnen beim Ziehen (Cache je Ansicht in lokalen Koordinaten).
  6a. M3: Der Export rechnet bei jeder Änderung an der Zeichnung neu, weil sein Schlüssel `edit_count` ist (nicht `model_edit_count` wie im Zeichnungsbereich, F8).
  6b. M6: Der Import-Dialog verwirft eine schon gelesene Datei, sobald sich `edit_count` ändert (auch bei Zeichnungsänderungen).
  6c. M7: Scheitert `make_body` (Körper aus dem Modell bauen) für die Zeichnung, steht der Fehler nur in der Statuszeile, nicht an der Ansicht.
  7. Doppelte Teilpunkte an Umkehrstellen von der Seite gesehener Kreise an der Quelle (`isect`) beheben, dann die Umgehung in `compute_view` entfernen.
  8. Freiform mit Pol in Seitenansichten unterstützen; Streifstrahlen nahe dem Gebietsrand als „unsicher“ behandeln (Aufgabe 7: `leaf_newton` ohne Rim-Neustart).
  9. Ziehen: Änderungen beim Loslassen zusammenfassen; `check_head` auf `CONTAINER_FORMAT` umstellen.
  10. Prüfung ergänzen: Verdeckt-auf-Verdeckt-Doppel, Überdeckungen unter 9 %, Doppel-Gestrichelt.
- *Niedrig*
  10a. DXF und Fusion 360: geschlossene Splines werden dort nicht als Profil (Fläche) erkannt (siehe Handtest). Abhilfe: viele kurze Bézier-Stücke oder Streckenzug für geschlossene Freiformkurven. Die Zeichnung zum Ansehen und Schneiden ist nicht betroffen, nur die Flächenbildung in Fusion.
  11. Kosmetik: Namen der Arbeitsfäden, Fehlertexte kürzen, reservierte Windows-Dateinamen, `drawing_view.rs` aufteilen, Testnamen und veraltete Kommentare.
  12. 3b (Maße, Schriftfeld, Mittellinien) und 3c (Schnitte) als eigene Teilprojekte.

**Laufzeiten und Testreihen**
- Teststand am Ende von Teilprojekt 11: **1759 bestanden, 0 fehlgeschlagen, 32 ignoriert in 214 Testprogrammen** (`cargo test --workspace -j 2`, Debug, 2026-10-15). Gesamtlaufzeit 14 Minuten 5 Sekunden (845 s) einschließlich Übersetzung. Zum Vergleich Ende von Teilprojekt 10: 1404 bestanden, 31 ignoriert, 193 Testprogramme. Der Aufruf mit `-j 2` ist nötig, weil die Windows-Auslagerungsdatei bei einem vollen parallelen Bau des Arbeitsbereichs zu klein war.
- Die längsten Testprogramme im Debug-Build: `stage8_merge_series` 97 s und `stage7_freeform_silhouette` 92 s (beide `planora-drawing`), danach Kernel-Tests mit 25 bis 29 s. Alle Stufen von `planora-drawing` zusammen brauchen im Debug-Build etwa 3,5 Minuten, im Release-Build etwa 35 s.
- Neu in Aufgabe 18: `hand_samples` (ein schneller Test, der zwei Proben in den Temp-Ordner schreibt, und der ausgeschaltete Test `write_samples_for_the_hand_test`, der alle 13 Proben schreibt und dabei die Formelwerte gegen die Bibliothek prüft; im Debug-Build 5,7 s) und neun Nachzüge in `stage1_model` (Maßstabsreihe, Papiergrößen A2 bis A0, Namen und Normansichten, Toleranzen, `view_extent`, Ausrichtung für 17 Eltern-Kind-Paare in beiden Projektionen, Entfernen mit Enkeln und unbekannter Kennung). Alle Sollwerte der Nachzüge wurden von Hand aus Spec und Plan gerechnet; keine Abweichung vom Code.
- `cargo fmt --all --check` und `cargo clippy --workspace --all-targets -- -D warnings` sind sauber; `cargo deny check` meldet advisories, bans, licenses und sources ok (nur Hinweise auf doppelte Versionen). Keine neue externe Abhängigkeit (nur der Schalter `float_roundtrip` von `serde_json`).
- Geschützte Pfade (`crates/planora-kernel`, `crates/planora-mesh`, `crates/planora-step`, `crates/planora-exchange`) sind bitgleich zu `945fa67` (`git diff --stat 945fa67 -- …` ist leer).

## Teilprojekt 12 – Maße und Mittellinien
Stand: 2026-10-16. Zeichnungen bekommen Maße (waagerecht, senkrecht, schräg, Durchmesser, Radius, Winkel, Maßketten und Bezugsmaße, Toleranzen) und Mittellinien. Maße hängen an der Modellgeometrie und rechnen sich nach Änderungen neu; ein Knopf schlägt die Grundmaße vor; SVG und DXF enthalten alles als einfache Linien, Pfeile und Texte. Das ist Teil 3b von vieren; Schriftfeld, Texte und Stückliste (3c) und Schnittansichten (3d) folgen. Spec: `specs/2026-10-15-masse-mittellinien-design.md` (der Abschnitt „Festlegungen der Umsetzung“ am Ende listet, was bei der Umsetzung anders wurde). Plan: `plans/2026-10-15-masse-mittellinien.md`.

**Entscheidungen mit dem Nutzer (2026-10-15):** Aufteilung des Rests von Teil 3 in Maße und Mittellinien, Blatt, Schnitte (M1); Maße von Hand und automatisch (M2); Maßarten linear, Durchmesser, Radius, Winkel, Ketten und Bezugsmaße, Toleranzen (M3); Maße hängen an der Modellgeometrie, gehen sie verloren, werden sie rot (M4); Maße nur in Normansichten (M5); Ausgabe als Linien, Pfeile und Text, keine echten DXF-Maße (M6); Mittellinien aus dem Modell berechnet und je Ansicht abschaltbar (M7); feste Darstellung nach DIN/ISO 129 in Blatt-Millimetern (M8); Passung nur als Text (M9); keine neue externe Abhängigkeit (M10). Bei der Planung kamen die Festlegungen P1 bis P13 dazu (Rechenwege, Dateiformat 5, Sperre des Exports bei verlorenen Maßen u. a.); sie stehen mit den Entscheidungen der Umsetzung in der Spec.

**Was geliefert wurde**
- `planora-drawing`: Maße und ihre Messung (alle Maßarten, wahrer Wert in Normansichten, benannter Fehler sonst), Mittellinien aus Kreisen, Zylindern, Kugeln und Ringen, Darstellung als Markierungen (Maßlinien, Hilfslinien, Pfeile, Text, Toleranztexte, Ketten, Bezugsmaße, rote verlorene Maße), Ausgabe in SVG und DXF (Ebenen `MASSE` und `MITTEL`, Linientyp `CENTER`), Vorschlag der Grundmaße. Ausgaben ohne Maße und ohne Mittellinien sind Byte für Byte wie vorher.
- `planora-feature`: Maße im Dokument (anlegen, ändern, löschen, geprüft), stabile Verweise auf Kanten und Flächen, Auflösen gegen die Körper.
- `planora-project`: Dateiformat 5 (nur wenn es Maße gibt oder eine Ansicht Mittellinien hat; Format 1 bis 4 laden unverändert).
- `planora-app`: Mittellinien-Schalter und „Maße vorschlagen“, Werkzeug „Maß“ mit Vorschau, Maße anklicken und ziehen, Fenster „Maß bearbeiten“, Browser-Abschnitt „Maße“, Export mit Maßen (Karte mit Zahl der Maße und Mittellinien-Stand, Sperre bei verlorenen Maßen, Zähler „außerhalb des Blatts“).
- Alle Texte deutsch in Laiensprache; Zeichnungen lassen sich auch in alten, schreibgeschützten Versionen ansehen (Maße dann ohne Ändern).

**Handtest (Stand 2026-10-16)**
- Die Proben erzeugt `cargo test --release -p planora-drawing --test hand_samples_dims -- --ignored` (Ordner `D:\Testumgebung\zeichnung-proben-masse`, abweichend über `PLANORA_SAMPLES_DIR`): `Quader_Masse`, `Zylinder_Masse`, `Bohrung_Masse`, `Winkel_Masse`, `Rundung_Masse`, `Kette_Bezug`, `Toleranzen` je als SVG und DXF in zwei Fassungen (`<Name>` mit den Maßen des Vorschlags, `<Name>_Hand` mit von Hand gesetzten Maßen) und `Verloren` (rote Maße, vom Programm normalerweise nicht exportierbar; hier ohne Sperre geschrieben). `ERWARTUNG.txt` nennt je Maß den Wert aus der Formel (beim Schreiben gegen die Messung geprüft) und den erwarteten Text auf dem Blatt, dazu die Prüfliste.
- Unabhängige Prüfung mit ezdxf (außerhalb des Repos): alle 15 DXF-Proben lesen sich ohne Fehler (`readfile OK`, Audit 0 Fehler, 0 Korrekturen). SVG von Hand angesehen: `Bohrung_Masse_Hand` im Browser zeigt Maßlinien mit Pfeilen, Texte, die Mittellinie und die gestrichelten verdeckten Kanten.
- **Handtest Fusion 360 (vom Nutzer, 2026-10-16):** `Bohrung_Masse.dxf` lässt sich fehlerfrei einfügen; Linien, Pfeile und Mittellinien kommen an, die Maßlinien als gewöhnliche Linien. Die Maßzahlen erscheinen nicht: Fusion 360 liest DXF-`TEXT` und `MTEXT` beim Einfügen in eine Skizze grundsätzlich nicht (bekannte Einschränkung von Fusion, kein Fehler der Datei). Entscheidung des Nutzers: Das DXF bleibt so; das Einlesen in Fusion ist ein Sonderfall, der aktuell nicht gebraucht wird. Mittellinien und Hilfslinien teilen dort die Skizzenflächen in Teilflächen. Nicht umgesetzt (nur falls es später gebraucht wird): Schalter „Zahlen als Linien“ (Strichschrift) und Schalter „DXF ohne Maße und Mittellinien“. Weiter ungeprüft: einzelnes Prozentzeichen (`%%%`), Rückstrich (`\U+005C`), Latin-1-Zeichen als `\U+XXXX`, `Ø`/`°`/`±` über `%%c`/`%%d`/`%%p`, roter Farbwert des verlorenen Maßes, Linientyp `CENTER`. Inkscape: Das SVG der Probe `Bohrung_Masse` sah richtig aus (Nutzer-Screenshot).

**Was nicht geht oder anders ist**

*Messung und Bezüge*
- **Nur Normansichten.** Die Art einer Ansicht lässt sich in der Oberfläche nicht ändern (man legt eine neue an); eine Isometrie mit Maßen, etwa aus einer veränderten Datei, lehnt die Dokumentprüfung mit Meldung ab.
- **Winkel** nur, wenn beide Kanten parallel zur Bildebene liegen (sonst wäre der Winkel verzerrt, Meldung statt Wert). Ein schräges Maß zwischen Punkten misst den Abstand im Bild (übliche Zeichnungsregel). Bei einem schräg stehenden Kreis oder einer Kante, die nicht quer zur Messrichtung steht, gibt es einen benannten Fehler.
- **Radius** nur an Kreis oder Bogen mit Blick in Achsrichtung und an Kugeln; ein Zylinder von der Seite gibt Durchmesser, kein Radius.
- **Nicht bemaßbar:** Ellipsen, Splines und Freiformflächen, ebene Flächen („Dieser Kantentyp lässt sich nicht bemaßen“ bzw. „Diese Fläche …“). Maße an Freiformflächen gehen nur über Kanten und Mittellinien.
- **Gleichstand** (Abstand der Kandidaten höchstens 1e-6 mm) gilt als mehrdeutig: das Maß wird rot mit Grund, nie ein geratener Wert.
- Die Toleranz für „Länge null“ kennt die Koordinatengröße nicht: bei weit vom Ursprung liegenden, gedrehten Körpern kann sie zu streng oder zu locker sein. Die Ansichtsbasis wird nur auf Endlichkeit geprüft.
- **Verlorene Maße** behalten den Schlüssel aus der letzten Auflösung, in der sich ihre Bezüge noch auflösen ließen; „Maße vorschlagen“ setzt daneben dasselbe Maß nicht noch einmal. Nur ein Maß, das in dieser Sitzung nie aufgelöst wurde (schon verloren aus einer Datei geladen), hat keinen Schlüssel und kann ein zweites Mal vorgeschlagen werden. Der Schlüssel beschreibt die Geometrie von damals (wie die letzte Lage). Die letzte bekannte Lage eines roten Maßes bleibt auch nach geänderten Bezügen an der alten Stelle. Ohne letzte Lage steht nur ein rotes „?“ in der Ansichtsmitte; Ziehen daran bleibt ohne Wirkung.
- Ein schräges Maß kippt die Seite seiner Maßlinie bei minimaler Neigung (genau waagerecht und senkrecht sind unberührt).

*Vorschlag*
- Nur Grundmaße (Gesamtmaße, Ø/R, Bohrungsmitten als Bezugsmaße), keine vollständige Fertigungsbemaßung; Gewinde und Fasen als Winkel mal Länge folgen.
- Der Umriss eines Zylinders oder einer Kugel am Rand zählt nicht als Bezug eines Gesamtmaßes. Bögen über 180° und Kugelkappen bekommen Ø statt R.
- Gleiche Bezugsebene mit gleichem Wert verschmilzt zu einem Maß (dokumentiert). Die Eckpunktsuche nutzt ein Raster nur für Enden nahe am Extrem (leicht anderes Toleranzband als die alte Suche).
- Der Abstand nach der Regel „10 mm plus n mal 7 mm“ zählt alle Maße derselben Ansicht und Richtung; von Hand versetzte Maße können Lücken oder Überlappungen lassen.
- Die Zufallsreihe der Prüfung ist schwach (200 Körper aus Teilprojekt 11).

*Mittellinien*
- Keine Mittellinien, wenn die Blickrichtung keine Weltachse ist (Isometrie). Überlappende kollineare Achsenlinien werden verbunden, konzentrische Kreuze zu einem.
- Verdeckte Bohrungen bekommen auch Mittellinien (bewusst akzeptiert). Gerundete Teile können mit Mittellinien der Ausrundungen überladen wirken (Produktfrage).
- Ein Torus-Kreuz überragt bei Ausrundungs-Tori (Hohlkehle) die größte Kreislinie. Der Achsbereich am Freiformrand wird nur an 65 Stellen abgetastet (nie zu lang).
- Die Randbegrenzung der Mittellinien aus der Spec (§3.4) ist **nicht** umgesetzt: sie stehen 3 mm über die Kanten hinaus, werden aber nicht am Ansichtsrand gekürzt.

*Darstellung und Ausgabe*
- Die **Textbreite ist nur geschätzt** (0,6 mal Texthöhe je Zeichen, P7); eine andere Schrift kann breiter sein, dann sitzen Texte dichter an Pfeilen. Bei zu langem Text ohne Maßlinie darunter bleibt der Text neben den innen liegenden Pfeilen.
- Der Text eines großen Ø (ab 25 mm auf dem Blatt) überdeckt bei 45°/135° eine Mittellinie. Der obere Toleranztext liegt 1,89 mm über dem Haupttext. Der Text eines roten Maßes kann weit über das Blatt ragen.
- Maße außerhalb des Blatts werden gezeichnet und in der Oberfläche und im Export gezählt, nicht gekürzt; auch lange rote Texte zählen als außerhalb.
- SVG: ein Bogen mit Radius null oder Ende vor Anfang wird still übersprungen; die Positionsgrenze `MAX_POS_MM` wird im SVG-Schreiber nicht geprüft; Text ohne `xml:space`.
- DXF: Latin-1-Zeichen werden als `\U+XXXX` geschrieben (reines ASCII), `%` als `%%%`, `\` als `\U+005C`, `^` als `^ `, Steuerzeichen als Leerzeichen. Ein Text über 255 Zeichen in dieser Schreibweise (`%` zählt dreifach) lässt den DXF-Export mit Meldung scheitern, SVG geht dann trotzdem. Ein leerer Text wird als leeres `TEXT` geschrieben; die Texthöhe wird im Schreiber nicht auf größer null geprüft. Die Pfeil-Dreiecke stehen in `svg.rs` und `dxf.rs` doppelt, `write_tables` hat acht Argumente.

*Dokument und Datei*
- Format 5 wird nur geschrieben, wenn es Maße gibt oder eine Ansicht Mittellinien hat; ältere Programmversionen melden dann „zu neu“. Format-4-Dateien laden mit ausgeschalteten Mittellinien (alte Zeichnungen bleiben unverändert).
- Format 4 speichert den Zähler der nächsten Maßnummer nicht: löscht man alle Maße (bei ausgeschalteten Mittellinien), beginnen die Nummern nach dem Laden wieder bei 1.
- Verweis-Hinweispunkte sind auf `2·MAX_COORD_MM` begrenzt (wie die übrigen Modellverweise), Rollennamen auf 64 Zeichen; größere Werte lehnen Anlegen und Laden mit klarer Meldung ab.

*Oberfläche*
- Die Maße werden nach einer Modelländerung im Oberflächenfaden aufgelöst (billig, bei 2000 Maßen aber spürbar); beim Ziehen werden die Markierungen je Bild neu berechnet, die Vorschau des Werkzeugs je Bild ohne Zwischenspeicher.
- Das Werkzeugfeld des Zeichnungsbereichs (früher: Werkzeugleiste) hat immer zwei Zeilen, damit die Fläche beim Wählen einer Ansicht nicht springt. Seit Etappe C schwebt es als Feld über dem Blatt; die Zeilen sind anders gruppiert (siehe „Teilprojekt 13 – Etappe C“).
- Werkzeug: Ein Klick auf den Umriss eines Zylinders misst in linearen Maßen bis zur Achse (Hinweis erscheint). Die Richtung einer Kette folgt automatisch dem größeren Wert; bei fast diagonalen Punkten kann sie kippen. Esc setzt eine Kette, sonst bricht es ab; Doppelklick wirkt nur bei Ketten. Scheitert eine Kette in der Mitte, bleiben die schon gesetzten Maße. Ein Maß wird nahe Ecken vor den Ansichtskanten getroffen; der Abstand 0 zählt als positiv; der kleinste Abstand beim Ziehen ist 1 mm (die Seite wechselt man derzeit nicht; ein Eingabefeld für den Abstand folgt). Zieht man die Maßlinie eines Glieds einer Kette oder von Bezugsmaßen, wandert die ganze Gruppe um dasselbe Stück mit (Text und Führungslinien ziehen weiter einzeln). Das Ziehen schreibt je Bild eine Änderung, damals gab es noch kein Rückgängig. Heute ist das Ziehen ein Schritt; was in einem Bild geschieht, ist ein Schritt, eine Kette wird nicht gesondert zusammengefasst.
- Die Meldung von „Maße vorschlagen“ verschwindet auch bei anderen Änderungen der Zeichnung. Bei noch nicht berechneter Ansicht fehlt ein eigener Hinweis.
- Fenster „Maß bearbeiten“: Eigener Text und Passung werden am Rand gekürzt, nur Leerzeichen gilt als „kein Text“; eine obere Abweichung kleiner als die untere und eine negative symmetrische Abweichung lehnt das Fenster rot ab (die Dokumentprüfung und der Dateileser prüfen das nicht, ältere oder veränderte Dateien laden also weiter). Zeilen zweier Maße mit gleichem Text unterscheiden sich nur durch Lage und Tooltip; Gruppenzeilen im Browser haben keine Wirkung; die Symbole sind von Hand gezeichnet und nicht im Bild geprüft; die Anzeige im Hauptfenster (`app.rs`) hat keinen Test.
- Export: Genannt wird nur das erste verlorene Maß. Der Satz endet mit „Bitte das Maß löschen und neu setzen.“, weil das Fenster „Maß bearbeiten“ keinen Bezug neu verknüpfen kann (früher „Bitte ändern oder löschen.“). Das DXF-Ergebnis kann scheitern (Textlänge), während SVG gelingt.
- Kleinkram im Code: Testfelder im Produktionszustand (`inputs`, `last_rect`, `last_cards`), doppelte Ergebnissuche im Maß-Fenster, ein Doku-Kommentar von `drawing_document` über dem falschen Helfer (`tests/drawings_format.rs`), tote `body`-Logik in `stage12_types.rs`, `ref_count` doppelt (freie Funktion und Methode), die Doku von `parallel()` nennt ein nicht vorhandenes `same_sense`.

**Nachbesserung nach der Gesamtprüfung (2026-10-16)**
- **Ansicht löschen mit Maßen:** Früher verschwanden die Maße einer gelöschten Ansicht (und ihrer abgeleiteten Ansichten) ohne Nachfrage, über „Ansicht löschen“ in der Werkzeugleiste sogar ganz ohne Rückfrage. Jetzt fragt das Programm immer, sobald Maße mitgelöscht werden („… und 3 Maße werden mit gelöscht.“); Werkzeugleiste und Browser gehen denselben Weg.
- **Aufwand der Bezüge:** Ein Bezug auf eine Kante speichert den Prüfpunkt beider Nachbarflächen. Der Prüfpunkt einer großen Fläche mit vielen Bohrungen ist teuer (Release-Build: 0,13 s bei 100 Bohrungen, 0,49 s bei 400), und er wurde für jede Kante neu berechnet. Jetzt merkt ein Zwischenspeicher (`RefCache` in `planora-feature`) Prüfpunkte, Kantenverwendungen und Achsbereiche je Durchgang (ein Vorschlag, ein Klick, eine Auflösung aller Maße); die Ergebnisse sind gleich (Vergleichstest), gezählt wird über einen Zähler je Thread. Gemessen (Release, Draufsicht einer Platte): „Maße vorschlagen“ bei 100 Bohrungen **18,7 s vorher, 0,41 s jetzt** (122 Maße), bei 400 Bohrungen 1,6 s (442 Maße; vorher nicht gemessen, nach der Rechnung mehrere Minuten). **Bleibt:** Ein Klick des Werkzeugs auf eine Kante der großen Fläche rechnet ihren Prüfpunkt einmal (0,13 s bzw. 0,49 s je Klick); eine Zylinderfläche aufzulösen kostet etwa 2,8 ms (Prüfpunkt plus Achsbereich), je Durchgang einmal.
- Schalter „Mittellinien“ in der Isometrie gesperrt, mit Erklärung am Schalter.
- Fenster „Maß bearbeiten“: Leerzeichen-Texte, obere/untere und negative Abweichung (siehe oben).
- Verlorene Maße: Gründe in Laiensprache („Die Fläche, an der das Maß hing, gibt es nicht mehr.“ statt Feature-Nummer und Rollenname), kein doppelter Vorschlag neben einem roten Maß, Exporttext „Bitte das Maß löschen und neu setzen.“
- Ziehen eines Glieds einer Kette oder von Bezugsmaßen verschiebt die ganze Gruppe.
- Rundungen: Die Führungslinie von R (und kleinem Ø) endet immer auf dem gezeichneten Bogen; liegt der eingestellte Winkel nicht auf dem Bogen, geht sie durch die Bogenmitte, Werkzeug und Ziehen halten sie am nächsten Bogenende. Ein großes Ø, das von Hand durch die Mitte auf einem Teilbogen gezeichnet wird, behält den eingestellten Winkel (seine Enden liegen dann nicht beide auf dem Bogen). Von Hand dicht am Umriss gesetzte senkrechte Maße können mit dem Text den Umriss berühren.
- Vorgeschlagene Maßlinien liegen 10 mm neben dem ganzen sichtbaren Umriss der Ansicht (nicht nur bei Rundungen, auch bei T- oder U-Profilen, deren äußerste Kanten kürzer sind als der Umriss). Liegen mehrere deckungsgleiche Kreiskanten auf demselben Kreis und ergeben zusammen einen Vollkreis, gilt der Bogen als voll und der Pfeil bleibt nicht auf dem Bogen gehalten (selten).
- Die Stelle eines Maßes in einer Kette ist auf höchstens 1999 begrenzt (Dateileser und Dokumentprüfung); größere Werte lehnt das Laden mit Meldung ab.

**Wie geprüft wurde:** Alle Stufen mit eigenen Formelmodellen (Werte aus den Eingabemaßen, nicht aus der Bibliothek), eigenen Lesern für SVG und DXF, einem Rücklesetest der Dateien, Prüfung feindlicher Dateien (NaN, riesige Zahlen, doppelte oder fremde Kennungen, Listen über der Grenze), einer Zufallsreihe und einem Mutationsbeweis für jeden Regressionstest (in den Aufgabenberichten). Die Proben prüfen beim Schreiben jeden Sollwert aus der Formel gegen die Messung.

**Folge-Aufgaben nach Priorität**
- *Hoch*
  1. Handtest in Inkscape und anderen Programmen für DXF-Text; für Fusion 360 gilt: Text wird nicht gelesen (bewusst hingenommen).
  2. Maße nach Modelländerungen im Hintergrund auflösen statt im Oberflächenfaden; Zwischenspeicher für Marken und Vorschau beim Ziehen. Prüfpunkte großer Flächen über mehrere Klicks des Werkzeugs hinweg merken (heute je Klick neu, siehe Nachbesserung).
- *Mittel*
  3. Mittellinien an den Ansichtsrand begrenzen (Spec §3.4); Überladung durch Ausrundungsachsen prüfen.
  4. ~~Rückgängig/Wiederholen für Maße, auch als ein Schritt je Ziehen und je Kette.~~ Erledigt im Teilprojekt „Rückgängig und Wiederholen“: Maße gehören zum Dokument und sind rücknehmbar, ein Ziehen ist ein Schritt (eine Kette nicht gesondert zusammengefasst).
  5. Textbreite aus der echten Schrift messen statt zu schätzen.
  6. Passungstabellen nach ISO 286, echte DXF-Maße, Maße an Ellipsen, Splines und Freiformflächen.
  7. Richtung einer Kette wählbar; Seite der Maßlinie beim Ziehen wechseln; Bezug eines roten Maßes im Fenster „Maß bearbeiten“ neu wählen.
  8. Vorschlag erweitern (Fasen als Winkel mal Länge, Gewinde, Randfälle bei Zylinder- und Kugelumriss).
- *Niedrig*
  9. Kosmetik: Text eines großen Ø bei 45°/135°, Toleranztext-Abstand, lange rote Texte kürzen, leeren DXF-Text und Texthöhe prüfen, Pfeilgeometrie in `svg.rs`/`dxf.rs` zusammenführen, `write_tables` aufräumen, Testfelder aus dem Produktionszustand, Doku-Kommentare und tote Logik.
  10. 3c (Schriftfeld, Texte, Stückliste) und 3d (Schnittansichten) als eigene Teilprojekte.

**Laufzeiten und Testreihen**
- Teststand am Ende von Teilprojekt 12: **2050 bestanden, 0 fehlgeschlagen, 33 ignoriert (nach den Korrekturen aus dem Handtest)** (`cargo test --workspace -j 2`, Debug, 2026-10-16). Die erste Vollreihe dauerte 15 Minuten 41 Sekunden einschließlich Übersetzung. Zum Vergleich Ende von Teilprojekt 11: 1759 bestanden, 32 ignoriert, 214 Testprogramme; das sind 291 Tests mehr, ein ignorierter mehr (`write_dimension_samples_for_the_hand_test`). Der Aufruf mit `-j 2` bleibt nötig (paralleler Vollbau scheiterte sporadisch an Speicher).
- Neu in Aufgabe 15: `hand_samples_dims` (ein schneller Test schreibt zwei Proben in den Temp-Ordner, der ausgeschaltete Test schreibt alle acht Proben in je zwei Fassungen und prüft dabei jeden Formelwert gegen die Messung; im Debug-Build unter einer Sekunde).
- `cargo fmt --all --check` und `cargo clippy --workspace --all-targets -- -D warnings` sind sauber; `cargo deny check` meldet advisories, bans, licenses und sources ok (nur Hinweise auf doppelte Versionen). Keine neue externe Abhängigkeit.
- Geschützte Pfade (`crates/planora-kernel`, `crates/planora-mesh`, `crates/planora-step`, `crates/planora-exchange`) sind bitgleich zu `945fa67` (`git diff --stat 945fa67 -- …` ist leer).

## Teilprojekt 13 – Graphit-Oberfläche, Etappe A (Farben, Schrift, Symbole)
Stand: 2026-10-16. Das ganze Programm hat das neue Aussehen „Graphit“ in Hell und Dunkel, die Schrift Montserrat und einheitliche Symbole. Der Aufbau des Fensters war in dieser Etappe unverändert (egui_tiles, Menüleiste, Tab-Leiste, Statuszeile, Zeichnungsleiste, Fenster „Skizze“); Etappe B hat ihn ersetzt (siehe unten). Das ist Etappe A von vieren (B Modell-Layout, C Zeichnung und Skizze, D Dialoge). Spec: `specs/2026-10-16-graphit-oberflaeche-design.md` (Abschnitt 10 „Festlegungen der Umsetzung Etappe A“). Plan: `plans/2026-10-16-graphit-etappe-a.md`. Zweig `feature/graphit-etappe-a`.

**Was geliefert wurde**
- Schriften: Montserrat (Regular, Medium, SemiBold; OFL 1.1) mit Roboto als Absicherung, Symbolschrift Phosphor Light (MIT), Lizenztexte und Prüfsummen in `assets/fonts/`. Test: jedes erlaubte Zeichen (Umlaute, ß, „“ – − · × ° ± ² ³ … Ø) ist in der Schrift vorhanden.
- Zentrales Theme (`theme.rs`): Palette in Hell und Dunkel, Abstände, Rundungen, Größen, Textstile, Feldrahmen und Zeilen-/Feld-Helfer, Fokusring; Kontrasttest für beide Themen.
- Symbole (`icons.rs`): 59 Symbole aus Phosphor Light (`Icon`) und 8 selbst gezeichnete (`HandIcon`: Bogen, Ellipse und Abhängigkeiten ohne Entsprechung in Phosphor); dazu die schon vorher gezeichneten Körper-, Feature-, Zeichnungs- und Maß-Symbole. Symbole in Menüs und Werkzeugzeilen.
- Farben aus Skizze, Würfel, Fehler und Warnung laufen über das Theme; ein Wächtertest verbietet neue hartkodierte Farben. Das Papier bleibt weiß mit schwarzen Maßen.
- Alle bestehenden Fenster, Leisten und Panels nutzen die neuen Helfer; ein Wächtertest verlangt `theme::text_field` für Einzeilen-Textfelder.

**Entscheidungen**
- Nebentext auf aktivem Grund darf 3,0 : 1 haben (Graphit selbst führt 3,9 : 1); alle anderen Paarungen 4,5 : 1.
- Dev-Abhängigkeit `skrifa` 0.44 für den Glyphentest (im `Cargo.lock` schon über `epaint`; `cargo deny` grün).
- Skizzenfenster bleibt 300 px breit.
- Werkzeug-/Symbolschalter, Tabs und Menüknöpfe sind randlos; Tastaturfokus als 2-px-Akzentring (Menüeinträge über `theme::entry`, auch im Menü „Zeichnung“ und bei Einträgen mit Schriftsymbol).
- Selbst gezeichnete Knöpfe (`icons::source_button`) und Abschnittsköpfe (`theme::section_header`) melden Art, Text und Zustand an Bildschirmleser (`widget_info`).
- Übergänge dauern 120 ms (`animation_time`).
- Durchmesser in der Skizze mit „Ø“ (vorher wegen der unzuverlässigen Glyphenabfrage von egui immer „D“).
- Ausführlich: Spec Abschnitt 10.

**Grenzen und offene Punkte**

*Kontrast*
- Nebentext (`text_secondary`) auf `bg_active`: nur 3,0 : 1 geprüft (dunkel etwa 3,9 : 1).
- Helle Palette weicht vom Entwurf ab: `bg_active` `#cfd0d5` statt `#dcdce0`, `bg_field` `#dadbdf`, `text_tertiary` `#66666e`, dunklere Achsenfarben und Fehlerrot `#c62828`, Skizzen-Auswahl `#a86a00` statt `#d9a400`, `axis_y` `#36903a`. Warnung `#8c5200`, Erfolg `#2a7020` und Markierung `#1a5cc4` sind dunkler, weil sie auch auf dem Fensterhintergrund `bg_viewport` `#e8e9ec` (Menü-, Tab- und Statusleiste, z. B. der Nur-Lese-Hinweis) 4,5 : 1 erreichen müssen. Die helle Vorschau sieht daher leicht anders aus.
- Der Kontrasttest prüft Text auf allen Flächen einschließlich `bg_viewport`. Ausgenommen bleibt der dunkle Graphit-Wert `text_tertiary` `#7d7d7d` (4,7 : 1 auf `bg_viewport`, 4,3 auf `bg_panel`, 4,2 auf `bg_section`); hell `#66666e` hat 4,7 / 5,7 / 5,2.
- Gesperrte Elemente: `disabled_alpha` 0,75 (egui-Vorgabe 0,5). Gesperrter Nebentext erreicht damit mindestens 3 : 1 auf Fenster, Feld, Abschnitt und Eingabe (hell 3,2 auf `bg_viewport`, auf `bg_field` und `bg_active` darunter; dort steht kein gesperrter Text).
- Helle Bodenachse (`ground_axis`) hat nur 2,45 : 1 (Linie, kein Text). Helle Achsen sind schwach.
- Skizzenfarben sind teilweise leicht zu verwechseln.
- `drawing_surround` hell borgt `bg_active`; `PAPER_SHADOW` ist auf dunklem Umfeld kaum sichtbar.
- 3D-Szene: Raster (`viewport_grid`), Achsen und Auswahl (`selection`, gewählte Kanten hell `#a86a00`) kommen aus dem Theme (`viewport::render_palette`); Flächen-, Kanten- und Hover-Farben sind weiter die alten Werte aus `planora_render` (Etappe B). `ground_axis` wird nicht genutzt (die Szene hat nur die Achsen X, Y, Z).
- Hyperlinks sind `text_primary`; Plain-Labels grau, Werte und Titel stark.

*Layout*
- Zeichnungsleiste im Viewport-Bereich bricht schon ab etwa 640 px Breite auf mehr als zwei Zeilen um (war vorher schon so). Etappe C.
- Das Fenster „Skizze“ bleibt 300 px breit und ist durch die 12-px-Abschnittsköpfe etwa 48 px höher als vorher.
- Felder (26 px) machen die Zeilen der Eigenschaftentabelle etwas höher.
- Fokusring am Menürand kann dort beschnitten sein.
- Dialog-Checkboxen sind unverändert; Browser-Kontextmenüs haben keine Symbole.
- Der Knopf „Maß“ ändert sich beim Überfahren nicht sichtbar.

*Symbole*
- Ungefähre Wahl: Symmetrisch, Muster, Kante projizieren, Deckungsgleich, Kegel, Kugel, Drehen, „zum Ende springen“. ViewFace ist für alle Ansichten gleich.
- Winkel (Phosphor) sieht etwas anders aus als Radius und Durchmesser (gezeichnet). Die gezeichneten Symbole haben 1,0-px-Feinlinien.
- `icons::label()` setzt 12 px fest (gegen `TextStyle::Button` noch nicht abgeglichen).

*Öffentliche Schnittstelle*
- Mehr `pub` als nötig: `theme::{window, section_header, SUB_HEADER_H, value, field, Toggle, menu_style, frameless, heading_font, label_font, value_font, kbd_font, text_field, text_field_enabled, focus_ring, entry, DISABLED_ALPHA, ANIMATION_TIME}`, `viewport::render_palette`, `sketch_view::overlay_text`, `icons::{atom, button, selectable, source_entry, source_button, …}`, `app::{file_menu, menu_bar}` (`doc(hidden)`). Vieles könnte `pub(crate)` sein. Aufräumen in Etappe D.

*Umfang der Prüfungen*
- Wächtertests (hartkodierte Farben, rohe Textfelder) scannen nur `src/*.rs` der obersten Ebene; mehrzeilige Textfelder prüft keiner. Der Selbsttest des Farb-Scanners deckt nicht alle Konstruktoren ab.
- Die meisten Testgeräte setzen nur die Schrift, nicht das Theme. Überlauf durch Abstände und Montserrat ist nur durch den Näherungstest `no_text_is_clipped_at_1400px` (Summe der Textbreiten) abgedeckt. Die Handliste unten ist deshalb wichtig.
- `ALLOWED` im Glyphentest ist handgepflegt; Roboto als Ersatzschrift wird nie gebraucht (Montserrat hat alle Zeichen).
- Zwischencommits der Aufgabe 5 wurden nicht einzeln kompiliert.

*Test mit Zeitabhängigkeit (behoben)*
- `export_step::the_window_stays_open_with_a_hint_while_step_is_written` (300 Quader, Abbrechen während des Schreibens) schlug seit dieser Etappe oft fehl (`Done` statt `Abgebrochen`): das Schreiben war fertig, bevor der Test abbrach. Gemessen (Debug): das Schreiben der 300 Quader dauert auf beiden Ständen rund 90–115 ms; das erste Bild des Fensters in einem frischen egui-Kontext dauerte vor Etappe A rund 43 ms, jetzt rund 95 ms (das zweite Bild 2,5 bzw. 4,8 ms). Die Mehrzeit ist das einmalige Rastern der Schriftzeichen im neuen Kontext (Montserrat in drei Schnitten und mehreren Größen); `theme::apply` läuft weiterhin nur bei einem Wechsel von Hell/Dunkel. Das Programm ist nicht betroffen (der Zeichensatz-Zwischenspeicher bleibt über die Bilder erhalten), nur das Rennen im Test kippte. Behoben, indem der Test über den neuen Testhaken `ExportDialog::set_step_writer` einen Schreiber einsetzt, der bis zum Abbrechen wartet und dann den echten `write_step_bodies` aufruft (der „Abgebrochen“ meldet und keine Datei hinterlässt); der Test braucht dafür nur noch 3 Quader. Die ganze Datei lief danach 12 von 12 Mal durch. Nach der Abschlussprüfung ebenso gehärtet: `the_cancel_button_belongs_to_the_running_write_not_to_the_chosen_format` (300 Quader gegen zwei Bilder), `cancelling_while_writing_says_so_and_keeps_the_old_file` (200 Quader, Abbruch gleich nach dem Start) und `closing_a_tab_during_a_running_step_write_…` (300 Quader; jetzt angehalten, die alte Datei muss genau erhalten bleiben) über den angehaltenen Schreiber; `closing_tabs_and_quitting_…` wartet auf die fertige Datei statt 1,5 s fest. Die übrigen Tests mit Arbeitsthreads (Zeichnungsansichten, Maß-Auflösung, Zeichnungs-Export) halten ihre Rechnung schon über Testhaken an oder warten mit Zeitgrenze auf ein Ergebnis.

**Handliste für den Nutzer** (`cargo run -p planora-app --release`, jeweils in Hell und Dunkel; Hell/Dunkel je Dokument-Tab umschalten)
1. Modell: Menüs (alle Einträge mit Symbol, nichts abgeschnitten), Tab-Leiste, Baum, Eigenschaften, Statuszeile; gewählte Teile, Würfel, Achsen gut lesbar?
2. Skizze: Fenster „Skizze“ (Werkzeug- und Abhängigkeitssymbole erkennbar, gezeichnete Symbole passen zu den anderen), Skizzenfarben, Auswahl.
3. Zeichnung: Leiste (bricht sie schmal um, ohne Teile zu verlieren?), Papier bleibt weiß mit schwarzen Maßen auch im dunklen Programm, Maße, rote verlorene Maße.
4. Dialoge: Export, Rundung/Fase, Boolesche Operationen, Extrudieren/Drehen, Maß bearbeiten, Speichern; Texte nicht abgeschnitten.
5. Zustände: Hover, Tastaturfokus (Tab-Taste; 2-px-Ring sichtbar auch auf Menüs und Tabs), gesperrte Elemente lesbar, Schalter und Auswahlfelder.
6. Montserrat: Breiten (lange Namen im Baum, Zahlen in Feldern, Umlaute, ß, „“, Ø, °, ±) ohne Kästchen und ohne Abschneiden.
7. Hell: Hover und Aktiv unterscheidbar, Nebentexte lesbar.
Beanstandungen sammeln; sie werden als Nachbesserung oder in Etappe B behoben.

**Zahlen (Stand 2026-10-16, Gesamtreihe)**
- `cargo test --workspace -j 2 --no-fail-fast`: 2087 bestanden, 0 fehlgeschlagen, 33 ignoriert, rund 14 Minuten (Debug). Der erste Lauf ohne `--no-fail-fast` brach am zeitabhängigen Test oben ab (Binärdatei `export_step`: 22 bestanden, 1 fehlgeschlagen), die Tests danach liefen dort nicht mehr (inzwischen behoben, siehe oben). `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` und `cargo deny check` sind grün; die geschützten Pfade (Kernel, Netz, STEP, Austausch) sind gegenüber 945fa67 unverändert.

**Folgearbeit**
- Etappe B (erledigt, siehe nächster Abschnitt): schwebendes Layout der Modell-Ansicht (oberes Feld, Baumfeld, rechte Felder, unteres Befehlsfeld, Meldungsfeld), sichtbare Mitte, Eingabe-Routing, Ersatz von egui_tiles. Der Fokusring am Menürand ist weiter nicht von Hand geprüft (Handliste B, Punkt 4).
- Etappe C: Zeichnung und Skizze als Felder, Zeichnungsleiste ohne Umbruch, Skizzenfeld rechts; Wächtertests auf Unterordner ausdehnen.
- Etappe D: Dialoge im neuen Rahmen mit neuer Breitenregel, Checkboxen, Kontextmenü-Symbole, Aufräumen der `pub`-Fläche und der toten Konstanten; Symbolwahl nach dem Handtest nachziehen.

## Teilprojekt 13 – Graphit-Oberfläche, Etappe B (schwebendes Layout)
Stand: 2026-10-16. Die Modell-Ansicht hat den neuen Aufbau: Der 3D-Bereich füllt das ganze Fenster, darüber schweben feste Felder (oben Menü und Tabs, links Projekte und Baum, rechts Eigenschaften oder Versionen, unten das Befehlsfeld, unten links die Meldungen); bei einer alten Version kommt ein schmales Hinweisfeld unter dem oberen Feld dazu. Das ist Etappe B von vieren (C Zeichnung und Skizze, D Dialoge). Spec: `specs/2026-10-16-graphit-oberflaeche-design.md` (Abschnitt 11 „Festlegungen der Umsetzung Etappe B“). Plan: `plans/2026-10-16-graphit-etappe-b.md`. Zweig `feature/graphit-etappe-b`.

**Was geliefert wurde**
- Kamera mit sichtbarem Bereich (`OrbitCamera::set_visible`/`clear_visible`): Der Mittelpunkt des Bildes wird in die Mitte der freien Fläche verschoben (Versatz der Bildmitte), ohne die Kamera-Mathematik umzubauen. Auswahl-Strahlen, Würfel, Skizzenblick und `restore_camera` folgen dem Versatz; ohne Versatz verhält sich alles wie vorher.
- Geometrie (`shell.rs`): `ShellState`, `ShellRects`, `layout()` rechnen die Lage aller Felder und die freie Fläche rein aus Fenstergröße und Konstanten, ohne Messen der Feldgrößen.
- 3D-Bereich im Vollbild (`viewport::show_in`) mit freier Fläche: Würfel rechts oben in der freien Fläche, Zeigerrouting über die Ebene unter dem Zeiger (nicht `is_pointer_over_egui`).
- Felder (`shell_panels.rs`): Area-Felder mit Rahmen aus `theme::panel_frame`, nicht verschiebbar; Menü und Tabs oben (Tab-Leiste rollt waagerecht und zeigt den aktiven Tab), Projekte und Baum links, Umschalter Eigenschaften | Versionen rechts, Meldungsfeld, Hinweisfeld für alte Versionen.
- Befehlsfeld: Chips Vereinigen, Abziehen, Schneiden (wählen die Operation im Booleschen Dialog vor), sechs Erzeugen-Symbole, Verrunden und Fase. Gemeinsame Funktionen mit dem Menü (`CreateKind`, `do_create`, `open_boolean`, `open_blend`), also genau eine Umsetzung; Hover-Hinweise nennen die Menübeschriftung.
- Verdrahtung: `egui_tiles` und `dock.rs` sind entfernt (samt Abhängigkeit, Lizenzvermerk und `Cargo.lock`-Block); Zeichnung läuft im freien Rechteck auf einem Hintergrund über das ganze Fenster; Dialoge liegen über den Feldern (`theme::window` mit `Order::Foreground`); Mindestfenstergröße 800 × 600.
- Einmaliges Nach-Einpassen (`fit_pending`): geladene Stände (Dokument öffnen, Version öffnen, Autosicherung, Wiederherstellen) und neue Tabs werden einmal in die freie Fläche eingepasst; ein Zoom des Nutzers bleibt danach stehen.
- Texte: „Im Feld Projekte …“ statt „Projekt-Panel“ in den Aufforderungen.

**Entscheidungen B1–B8** (Entwurfsverfasser, der Nutzer kann sie kippen; Wortlaut in der Spec, Abschnitt 11)
- B1 Rand 16 px, Abstand zwischen Feldern 12 px (Graphit nennt 24 px Rand).
- B2 Die freie Fläche kommt deterministisch aus Konstanten, nicht aus gemessenen Feldgrößen (keine Rückkopplung).
- B3 Positionen aus `ctx.content_rect()` und Konstanten (`fixed_pos`); Felder mit unbekannter Breite verwerfen den ersten Größen-Durchgang.
- B4 Felder `Order::Middle` und nicht verschiebbar; Dialoge und Skizzenfenster `Order::Foreground`.
- B5 Zeiger-Routing über `response.hovered()`, `hover_pos()` und `rect_contains_pointer`, nie `is_pointer_over_egui`.
- B6 Der Modusschalter oben rechts entfällt (es gibt keine Modi dafür); das Befehlsfeld ist nur eine Abkürzung zu vorhandenen Menüaktionen.
- B7 Der Hinweis „Alte Version vN – schreibgeschützt …“ steht als eigenes Feld unter dem oberen Feld (Farbe `warning`).
- B8 Die Zeichnungsansicht bleibt mit ihrer alten Werkzeugleiste im freien Rechteck; das Skizzenfenster bleibt ein verschiebbares Fenster (300 px), Startposition rechts neben dem linken Feld (x = 290).

**Entscheidungen der Umsetzung (Rulings)**
- `ray()` rechnet die Inverse in f64: Die Genauigkeit 1e-4 mm aus der Aufgabe ist mit f32 nicht erreichbar (schon ohne Versatz 1,6e-4 mm, mit Versatz 1,1e-3 mm). Strahlen sind damit ohne Versatz nicht mehr bitgleich, nur genauer; `view_proj` und `project` bleiben ohne Versatz bitgleich. Rückfall bei Bedarf: f32 mit Toleranz 2e-3 mm.
- Das Befehlsfeld verhält sich im Skizzenmodus wie das Menü (die Skizze wird beendet, dann öffnet sich der Dialog); es wird dort nicht gesperrt. Es verschwindet nie (Mindestbreite etwa 250 px, gemessen aus drei Chips, Trennstrich und Rand); bei wenig Platz rollt es waagerecht.
- Das Meldungsfeld wächst bis zu 3 Zeilen (`STATUS_ROWS`) und ist so breit wie der Platz links vom Befehlsfeld; längerer Text wird mit „…“ gekürzt, der volle Text steht beim Überfahren.
- Das Hinweisfeld hebt die Oberkante der freien Fläche um `BANNER_H` (40) + Abstand (12) an, auch bei einzeiligem Hinweis; der Hinweis ist so breit wie der Platz zwischen den Spalten (bis zu zwei Zeilen).
- Mindestfenstergröße 800 × 600 (`with_min_inner_size`).
- `fit_pending`: Nach dem Laden und bei neuen Tabs passt das erste Bild der 3D-Ansicht einmal in die freie Fläche ein. Im Skizzenmodus und in der Zeichnung bleibt die Marke stehen und wird nachgeholt.
- Schreibschutz wie vorher: Baum und Eigenschaften gesperrt, Projekte und Versionen nicht; Zeichnung `editable = !read_only`.
- Meldungs- und Hinweisfeld werden nach den anderen Feldern mit frischem Stand angeordnet (Tabwechsel, neue Meldung ohne Zwischenbild).

**Grenzen und offene Punkte**

*Kleine Fenster*
- Unter etwa 640 px Breite überlappen das untere Feld und die Seitenspalten; unter etwa 360 px Höhe überlappen Felder ebenfalls (das obere Feld überdeckt seit `layout_with_top_min` keine Spalte mehr, solange das Fenster nicht schmaler als sein Mindestmaß ist). Seit der Mindestfenstergröße 800 × 600 betrifft das nur ein Fenster, das von außen kleiner gezogen wird.
- Unterhalb von etwa 990 px Breite passt das obere Feld (Menü und Tabs, mindestens rund 431 px) nicht mehr zwischen die Spalten: Es steht dann mittig über den Spalten, und die Spalten beginnen darunter (`shell::layout_with_top_min`). Bei 800 × 600 ist das so; ab etwa 1100 × 700 füllt das obere Feld den Platz zwischen den Spalten wie vorher. Das Befehlsfeld rollt bei 800 × 600 waagerecht.
- Kein Tastatur-Zoom der Oberfläche (Strg und Plus oder Minus ist abgeschaltet, `PlanoraApp::setup_context`), weil die Felder in Punkten auf 800 × 600 ausgelegt sind; ein Zoom ließe die Mindestgröße wirkungslos werden.
- Die linke Spalte klemmt ihre Unterkante mit einer festen Meldungshöhe (`STATUS_H` 32). Das Meldungsfeld mit drei Zeilen reicht bei 800 × 600 bis etwa 3 px unter die freie Fläche; ein Überlappen mit der linken Spalte ist dort nicht getestet. Die Garantie gilt nur, solange das Meldungsfeld höchstens 68 px hoch ist.
- `shell::layout()` ignoriert `ShellState`: Die Seitenspalten sind immer vorhanden, auch wenn Projekte oder Versionen ausgeblendet sind (nur der Inhalt ändert sich). Bei ausgeblendetem Feld ist die freie Fläche daher nicht größer.
- Das Hinweisfeld ist 40 px hoch, auch wenn der Hinweis nur eine Zeile braucht (12 px mehr als nötig).

*Skizzenfenster* (seit Etappe C überholt: das Fenster „Skizze“ gibt es nicht mehr)
- Das Fenster „Skizze“ ist bei 1400 × 900 so hoch, dass egui es nach oben schiebt (Oberkante 81 statt 90, Unterkante am Fensterrand); es überdeckt dann Meldungs- und Befehlsfeld, bleibt aber vorn und verschiebbar. Lage und Höhe regelt Etappe C oder D.
- Wird eine Skizze unmittelbar nach dem Laden geöffnet, bevor die 3D-Ansicht je ein Bild gezeichnet hat, passt `open_sketch` noch mit der Kamera ohne freie Fläche ein: Die Mitte stimmt, die Größe kann bei kleinen Fenstern etwas über die freie Fläche hinausragen. In der App kommt das praktisch nicht vor.
- Der Fall „die Skizze bleibt offen, weil der Entwurf ungültig ist“ beim Klick im Befehlsfeld ist nicht getestet.
- Im Skizzenmodus ist das zweite `fit_camera` nach dem Skizzen-Overlay nur durch einen Test des Seitenverhältnisses bei symmetrischer freier Fläche abgedeckt; ein Zeigerrouting-Test im Skizzenmodus deckt Klick und Mausrad auf den Feldern ab.

*Befehlsfeld und Symbole*
- Verrunden und Fase teilten sich dasselbe Symbol (seit Etappe C hat die Fase ein eigenes).
- Es gibt keinen Menüeintrag „Verschieben“, deshalb auch keinen Knopf im Befehlsfeld.
- Chip-Farben kommen aus der Palette (`theme::chip_fill`) in Hell und Dunkel; ob sie auf der echten Oberfläche gut unterscheidbar sind, zeigt die Handliste.

*Kamera und Einpassen*
- Die Erkennung „kein sichtbarer Bereich“ in `frame_bounds` (Versatz 0 und Anteil 1) ist geraten: Wer `visible_aspect` ohne Versatz und Anteil setzt, wird dort ignoriert (latent, im Programm nicht erreichbar).
- `set_viewport` überschreibt `visible_aspect` bei Versatz 0; Aufrufer mit freier Fläche müssen `set_visible` nach `set_viewport` aufrufen (die App tut das pro Bild).
- Ein Kameratest (Test (e), ungültiger sichtbarer Bereich) kann nicht scheitern.
- Das Nach-Einpassen feuert beim ersten Modellbild mit einer freien Fläche von mindestens 1 × 1 px; bei der Mindestfenstergröße ist das unkritisch.

*Texte, Felder, Schnittstelle*
- „Panel“ steht noch in älteren Abschnitten dieser Datei und in einzelnen Hover- und Fehlertexten; die Oberfläche selbst sagt „Feld“.
- Der Fokusring am Menürand (offen aus Etappe A) ist nicht von Hand geprüft.
- Neue `#[doc(hidden)] pub`-Testhaken: `PlanoraApp::{for_tests, ws_mut, shell_mut}`, `shell_panels::{last_layout, field_rects, bottom_buttons, discard}`, `PlanoraApp::setup_context`. Aufräumen in Etappe D zusammen mit den übrigen `pub`-Flächen.
- Die Tests treiben das echte `eframe::App::ui` mit `Frame::_new_kittest()`; sie sehen kein echtes Rendering (Zeichnen des 3D-Bereichs unter den Feldern, Flackern beim Fensterziehen, Mindestgröße im echten Fenster). Deshalb die Handliste.
- Die Zeitabhängigkeit aus Etappe A (das erste Bild in einem frischen egui-Kontext ist langsamer) bleibt ein Risiko für Tests mit Arbeitsthreads.

**Handliste für den Nutzer** (`cargo run -p planora-app --release`, im Modell-Tab; jeweils in Hell und Dunkel)
1. Fenstergrößen: 800 × 600 (Mindestgröße, kleiner ziehen geht nicht), 1100 × 700, ein großes Fenster. Felder bleiben im Fenster, überlappen einander nicht, springen beim Ändern der Größe nicht und flackern nicht; links und rechts rollen die Felder, wenn der Inhalt höher ist.
2. Hell und Dunkel (je Dokument-Tab umschalten): Rahmen, Chips im Befehlsfeld, Hinweisfeld, Meldungsfeld gut lesbar.
3. Alle Menüs (Datei, Erzeugen, Bearbeiten, Ansicht, Zeichnung): öffnen sich unter dem oberen Feld, nichts abgeschnitten, alle Einträge und Tastenkürzel wie vorher.
4. Fokusring am Menürand mit der Tab-Taste.
5. Tabs: neuer Tab, Tab wechseln, Tab schließen; viele Tabs (zum Beispiel 20): die Leiste rollt, der aktive Tab kommt ins Bild.
6. Ansicht: Projekte und Versionen ein- und ausblenden; Umschalter Eigenschaften | Versionen rechts; Strg+O blendet Projekte ein.
7. Das Modell dreht, wählt und zoomt nicht, wenn die Maus über einem Feld ist (Klick, Ziehen, Mausrad); ein im 3D-Bereich begonnenes Ziehen läuft über Felder weiter.
8. Dialoge (Export, Rundung/Fase, Boolesche Operationen, Speichern) liegen über den Feldern und gehen nicht hinter ein Feld, wenn man auf ein Feld klickt.
9. „Alles einpassen“ zentriert das Modell in der Mitte zwischen den Feldern; ebenso nach neuem Tab, Dokument öffnen und Version öffnen.
10. Dokument öffnen aus dem Feld Projekte; Skizze öffnen und schließen (die Ansicht kommt zurück, wie sie war); das Skizzenfenster erscheint rechts neben dem linken Feld und lässt sich verschieben (seit Etappe C: Leiste links, Feld rechts, Fuß unten).
11. Zeichnung: Ansicht „Zeichnung“ wählen; das Blatt liegt im freien Bereich zwischen den Feldern, die Zeichnungsleiste ist wie vorher (seit Etappe C: Werkzeugfeld, Ansichtsfeld, Hinweisfeld).
12. Befehlsfeld: Chips Vereinigen, Abziehen und Schneiden öffnen den Booleschen Dialog mit der gewählten Operation; die Erzeugen-Symbole, Verrunden und Fase öffnen die gleichen Dialoge wie das Menü; Hover-Hinweise; bei 800 × 600 rollt das Feld waagerecht.
13. Schreibgeschützte alte Version öffnen: Hinweis unter dem oberen Feld, Baum und Eigenschaften gesperrt, freie Fläche rutscht nach unten.
14. Lange Fehlermeldung: Das Meldungsfeld zeigt bis zu drei Zeilen, überdeckt nichts, voller Text beim Überfahren.
Beanstandungen sammeln; sie werden als Nachbesserung oder in Etappe C und D behoben.

**Zahlen (Stand 2026-10-16, Gesamtreihe)**
- `cargo test --workspace -j 2 --no-fail-fast`: 2163 bestanden, 0 fehlgeschlagen, 33 ignoriert, rund 16 Minuten (Debug), kein sporadischer Fehler. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` und `cargo deny check` sind grün; die geschützten Pfade (Kernel, Netz, STEP, Austausch) sind gegenüber 945fa67 unverändert.

**Handprüfung Haltepunkt 2 (2026-10-16): Befund und Behebung**
- Befund 1: Mit gewähltem Körper war das rechte Feld breiter als seine Spalte (341 statt 250 px bei 1400 × 900). Werte am rechten Rand („≈ 2…“, „20.“, „40.“) waren am Fensterrand abgeschnitten, und das Feld lag über dem Navigationswürfel (egui schob das zu breite Feld zurück ins Fenster). Ursache: Die Eigenschaften standen in einem `egui::Grid`; dort brechen Texte nicht um, und die drei Zahlenfelder der Position (mit „ mm“) standen nebeneinander. Das Raster war breiter als die Innenbreite, die senkrechte ScrollArea (`auto_shrink` waagerecht aus) wuchs mit, und mit ihr das Feld. Montserrat ist breiter als Roboto, deshalb fiel es jetzt auf.
- Behoben: (a) Die Seitenfelder zeichnen ihren Inhalt in einer eigenen Fläche fester Breite (`shell_panels::side_column`), deren Größe nicht ins Feld zurückwirkt; was trotzdem zu breit wäre, wird am Innenrand des Rahmens abgeschnitten. Beide Seitenfelder sind damit immer genau `PANEL_WIDTH` breit. (b) Die Eigenschaften (Körper, Feature, Netz-Körper) stehen in Zeilen „Bezeichnung – Wert“: passt beides nebeneinander, steht der Wert rechtsbündig daneben, sonst darunter und umbrochen. Nichts wird gekürzt. (c) Position und Drehung: Die Einheit steht in der Bezeichnung („Position x / y / z in mm“, „Drehung x / y / z in Grad“), die drei Zahlenfelder teilen sich die Breite; passt eine Zahl nicht in ihr Drittel, stehen sie im nächsten Bild untereinander mit x, y, z davor (während des Ziehens bleibt die Anordnung).
- Befund 2 (beim Test gefunden): Bei 800 × 600 war das obere Feld (Menü und Tabs, rund 430 px) breiter als der Platz zwischen den Spalten und lag über dem Umschalter „Eigenschaften | Versionen“. Behoben: `shell::layout_with_top_min` bekommt die Mindestbreite des oberen Felds (Menü, Trennstrich, 120 px Tabs, Rand); passt sie nicht zwischen die Spalten, steht das obere Feld über den Spalten, und die Spalten beginnen darunter. Bei 1100 × 700 und größer ändert sich nichts.
- Befund 3: In der Tab-Leiste lag ein dicker weißer Rollbalken über den Tab-Namen. Ursache: egui-Vorgabe (10 px breit, Farbe = Schriftfarbe der Knöpfe, beim Überfahren `text_primary`, voll deckend). Behoben: `theme::scroll_style()` (in `theme::apply`) für alle Rollbalken: schwebend (belegt keinen Platz), in Ruhe unsichtbar, keine Spur, 6 px über der Fläche, 8 px über dem Griff, abgerundet, 2 px vom Rand. `theme::scroll_area` färbt den Griff aus der Palette (`text_tertiary` zu 60 % über der Fläche, `text_secondary` beim Überfahren oder Ziehen). Die Tab-Leiste reicht für ihren Balken in den unteren Rand des oberen Felds, der Balken liegt also unter den Tab-Namen (das Feld bleibt 48 px hoch). Das Befehlsfeld unten rollt weiter ohne sichtbaren Balken (wie bisher).
- Tests: `tests/side_field_width.rs` (gewählter Körper, großer Körper mit langen Zahlen, Feature, Fläche, Kante, Netz-Körper, langer Name; 1400 × 900, 1100 × 700, 800 × 600; Hell und Dunkel: Feldbreite ≤ 250, im Fenster, kein Text über dem rechten Rand, keine Texte übereinander, kein Feld über dem Würfel oder über einem anderen Feld); `tests/theme.rs` (Rollbalken-Stil und Griff-Farben in beiden Themen); `tests/shell_panels.rs` (Balken der Tab-Leiste in Ruhe unsichtbar, beim Überfahren schmal, in der Palettenfarbe, nicht über einem Tab-Namen).
- Grenzen: Rollbalken in egui-eigenen Bereichen (lange Aufklapplisten, Menüs) haben den neuen Stil, aber die egui-Farben (Schriftfarbe der Knöpfe, beim Überfahren `text_primary`, also hell); sie sind dort selten. Das Aussehen ist nur über Tests geprüft, nicht am Bildschirm: Handliste Punkt 1 (rechtes Feld mit gewähltem Körper bei allen drei Größen), Punkt 5 (Rollbalken der Tab-Leiste) und ein Blick auf die Rollbalken links und rechts bei langem Baum. Der Navigationswürfel selbst (Aussehen) wird auf Wunsch des Nutzers in Etappe C überarbeitet.
- Befund 4 (offen, Etappe C): Bei Fensterbreiten unter etwa 1000 px liegt die Startposition des Skizzenfensters (x = 290, Breite 300) über dem Navigationswürfel (bei 800 × 600 der Würfel bei x 414 bis 510, y 88 bis 184). Das Skizzenfenster wird in Etappe C ohnehin zum Feld umgebaut.
- Befund 5 (offen, Etappe D): Öffentliche Fläche aufräumen: `app::{CreateKind, create_blocked, do_create, open_boolean, open_blend}` könnten `pub(crate)` sein; `viewport::show`, `shell::layout`, `tabs::show` und `versions_panel::show` werden nur noch von Tests benutzt.

**Folgearbeit** (Etappe C ist umgesetzt, siehe den Abschnitt „Etappe C“ unten)
- Etappe C: Zeichnung und Skizze als Felder, Zeichnungsleiste ohne Umbruch, Skizzenfeld rechts statt Fenster (damit entfällt die Platzierung des Skizzenfensters); `layout()` mit `ShellState`; Wächtertests auf Unterordner ausdehnen; Gestaltung des Navigationswürfels (vom Nutzer nach Haltepunkt 2 zurückgestellt; Wunsch des Nutzers: das Aussehen des Würfels wird überarbeitet); Skizzenfenster nicht mehr über dem Würfel (Befund 4).
- Etappe D: Dialoge im neuen Rahmen mit neuer Breitenregel, Checkboxen, Kontextmenü-Symbole, Aufräumen der `pub`-Fläche (auch der Testhaken oben und Befund 5) und der toten Konstanten, eigenes Fase-Symbol; Symbolwahl nach dem Handtest nachziehen.

## Teilprojekt 13 – Graphit-Oberfläche, Etappe C (Zeichnung, Skizze, Würfel)
Stand: 2026-10-16. Zeichnung und Skizze haben den neuen Aufbau mit schwebenden Feldern, der Navigationswürfel ist neu gestaltet, und die Fase hat ein eigenes Symbol. Das ist Etappe C von vieren (D: Dialoge). Spec: `specs/2026-10-16-graphit-oberflaeche-design.md` (Abschnitt 12 „Festlegungen der Umsetzung Etappe C“). Plan: `plans/2026-10-16-graphit-etappe-c.md`. Zweig `feature/graphit-etappe-c`.

**Was geliefert wurde**
- Navigationswürfel (`planora-render/viewcube.rs`, `planora-app/viewport.rs`): 26 anklickbare Bereiche (6 Flächen, 12 Kanten, 8 Ecken), Home-Knopf, Hervorhebung der aktuellen Ansicht in der Akzentfarbe, Name der Ansicht unter dem Würfel, Beschriftung auf den Flächen (verzerrt und mitgedreht). Neu in der Kamera: `home()` und `set_view_region(region)`; die Kamera-Mathematik selbst ist unverändert. Alte Funktionen (`ViewFace`, `cube_faces`, `cube_hit_test`, `set_view`) und ihre Tests bleiben.
- Skizzenmodus: Das Fenster „Skizze“ entfällt. Links eine Leiste mit den 9 Werkzeugen als Symbole (44 px breit, senkrecht mittig), rechts ein Feld (Abhängigkeiten, Maßeingabe, Bearbeiten, Spiegeln/Muster, Liste), unten ein Fußfeld (Skizzenstatus, Werkzeughinweis, „Skizze beenden“). Das Menü Erzeugen/Bearbeiten ist unverändert und beendet die Skizze wie bisher.
- Zeichnungsmodus: Das Blatt füllt das ganze Fenster; Werkzeugfeld oben in der Mitte (zwei feste Zeilen), Ansichtsfeld rechts (Maßstab, Verdeckte Kanten, Mittellinien, Ansicht löschen), Hinweisfeld unten (Maßhinweise, Meldungen, Werkzeughinweis). Linke Spalte und Statusfeld wie im Modell.
- Felder: Jedes Feld hat am Rand ein unsichtbares Hover-Widget, damit das Mausrad am Feldrand nicht mehr zur Skizze oder zum Blatt durchgreift (`shell_panels::field`, gilt auch für die Felder aus Etappe B).
- Fase-Symbol: `FeatureIcon::Chamfer` (gerade Schräge, handgezeichnet) in Menü, Befehlsfeld und Baum; Verrunden behält `Blend`.
- Geometrie: `shell::ShellMode`, `layout_in_mode`, `STRIP_W`, `TOOL_H`; `ShellRects` kennt `strip` und `tool`; `DrawingLayout.paint` (volles Malrechteck) neben `canvas` (Einpass-Rechteck).

**Entscheidungen C1–C10** (Entwurfsverfasser; C1 mit Entscheidung des Nutzers; Wortlaut in der Spec, Abschnitt 12)
- C1 (Nutzer) Würfel: 26 Bereiche, Home, Hervorhebung nur bei Blick auf höchstens 2° zu einem Bereich, Achsenfarben leicht getönt, Größe 100 px (ursprünglich 84, in Etappe D geändert, D5), Beschriftung auf der Fläche, Name unter dem Würfel.
- C2 Home = Isometrisch und „Alles einpassen“.
- C3 Klick: Fläche = Ansicht auf diese Fläche; Kante = Blick entlang der Kantennormalen; Ecke = Blick auf die Ecke im Isometrie-Winkel.
- C4 Beschriftungsausrichtung: Seitenflächen „oben“ = +Z, Oben-Fläche „oben“ = +Y, Unten-Fläche „oben“ = −Y, „rechts“ = oben × Normale (nie gespiegelt).
- C5 Skizzenmodus: Leiste links ersetzt das linke Feld, Feld rechts ersetzt Eigenschaften/Versionen, Fußfeld ersetzt das Befehlsfeld.
- C6 Zeichnungsmodus: Werkzeugfeld (`TOOL_H` 76) unter dem oberen Feld, Ansichtsfeld ersetzt Eigenschaften/Versionen, Hinweisfeld ersetzt das Befehlsfeld.
- C7 `drawing_view::show_with` und `sketch_panel::show` zeichnen ihre Felder selbst; ohne Shell im `ctx` rechnen sie das Layout aus `ctx.content_rect()`.
- C8 Das Befehlsfeld gibt es nur im Modell; in Skizze und Zeichnung erreichen Befehle den Nutzer über das Menü.
- C9 Das Skizzenfenster entfällt (damit erledigt sich „Skizzenfenster überdeckt Würfel/Status“ aus Etappe B).
- C10 Fase bekommt ein eigenes Symbol.

**Entscheidungen der Umsetzung (Rulings)**
- `current_region` behandelt `|pitch| ≥ MAX_PITCH − 1e-4` als genau senkrecht. Die Kamera stoppt 0,573° vor dem Pol, und ohne dieses Einrasten wäre der Rundlauf Oben/Unten (Klick, dann `current_region`) nicht möglich. `Edge{a, b}` muss die Reihenfolge Z, Y, X einhalten (es gibt keinen normalisierenden Konstruktor).
- Würfel Variante C in Größe B (zuerst 84 px, seit Etappe D 100 px). Isometrie = Ecke Oben-Vorne-Rechts; die drei Nachbarecken sind dort als schräge Dreiecke sichtbar, ihre Kanten nicht (geometrisch richtig).
- Home-Knopf links neben dem Würfel, oben bündig, 2 px Überlapp mit dem Würfelfeld (damit er in der freien Fläche bleibt). Würfel und Home nehmen `Sense::click_and_drag`, damit ein Zug auf dem Würfel nicht dreht.
- Beschriftung als egui-Galley-Netz, das mit den Achsen der Fläche abgebildet wird (kein Rückfall auf Strichschrift nötig). Beschriftung ab `facing` 0,3 (`LABEL_MIN_FACING`), voll deckend ab 0,5; Stücke selbst ab 0,15, voll ab 0,3. Das längste Wort füllt 85 % der Flächenbreite (Schrift etwa 7–8 px). (Etappe D: Schwellen jetzt 0,5/0,7, Füllung 0,95, Mindestschrift 9 pt.)
- Hover-Farben: normale Bereiche `cube_hover_fill`, aktuelle Ansicht `highlight_hover` (Akzent 12 % zu Weiß).
- Das Fußfeld im Skizzenmodus darf breiter sein als der Platz zwischen den Spalten (Obergrenze `Fensterbreite − 2·(Rand + 44 + Abstand)`); der Status schrumpft. Sonst überlief bei 800 × 600 der Status den Knopf „Skizze beenden“.
- Der Schalter heißt „Waagerecht/senkrecht erkennen“ und steht zweizeilig (einzeilig 201 px, im Feld bleiben 188 px). Der Nutzer prüft die Beschriftung am Bildschirm; Hover-Text `AUTO_AXIS_HINT`.
- Löschen in der Abhängigkeitsliste ist ein Symbolknopf (Tooltip und `widget_info` „Abhängigkeit löschen“); „anzeigend“ steht bei Maßen in einer zweiten Zeile. Die Maßeingabe stapelt (Zahlenfeld, darunter OK/Abbrechen) und rollt sich selbst ins Bild.
- Feldrand-Hover-Widget für alle Felder (um `interact_radius` vergrößertes, unsichtbares Widget in `shell_panels::field`); kein toter Streifen, außerhalb des Felds bleibt die Fläche darunter erreichbar. Mausrad über Würfel und Home-Knopf zoomt (Modell und Skizze; Zoom läuft nach den Würfel-Knöpfen, `viewport::show_in`).
- Zeichnung: `DrawingLayout.canvas` = `free` (Einpass- und Zentrierrechteck), `paint` = volles Fenster (Blatt, Umgebung, Schatten, Eingabefläche). Werkzeugfeld mit zwei festen Zeilen: Zeile 1 „3D-Modell“, „Zeichnung“, |, „Ansicht hinzufügen“; Zeile 2 „Blatt einpassen“, −, +, |, „Maße vorschlagen“, „Maß“, Modus. Die Gruppierung weicht vom Brief ab, weil die alte Zeile 1 bei 1100 × 700 (518 px Innenbreite) nicht passte. Die Zeilen wachsen seitlich in einer Rollfläche mit sichtbarem dünnen Balken bei Überlauf.
- Schreibgeschützt hebt die Oberkante der freien Fläche um 52 (Hinweisfeld 40 + Abstand 12) an, nicht um 40; das Werkzeugfeld liegt unter dem Hinweis.
- Hinweisfeld höchstens 2 Zeilen: Maßhinweise teilen sich Zeile 1 (mit Meldung: halbe Zeile), werden mit „…“ gekürzt und zeigen den vollen Text beim Überfahren.
- Fehlt der Platz für ein Blatt (zu kleines Fenster), wird die Umgebung trotzdem über das Fenster gemalt.
- Test-Fenster der Maßtests (`dimension_view`, `dimension_dialog`) 1600 × 1000 statt 1200 × 800: Die Einpassfläche ist kleiner geworden (2,0 statt 3,1 px/mm bei 1200), das hätte handgewählte Pixel-Züge gebrochen.
- Fase: `FeatureIcon::for_blend(chamfer)` als gemeinsame Auswahl; die Baumzeile entscheidet über `BlendArt::is_chamfer`.
- Geschützte Pfade (Kernel, Netz-Quelltext, STEP, Austausch) unverändert gegenüber 945fa67; `planora-render` nur mit Zusätzen.

**Grenzen und offene Punkte**

*Würfel*
- Patent: Gestaltung und Verhalten des Würfels (26 Bereiche, Home) ähneln dem ViewCube von Autodesk; vor einer Veröffentlichung auf Patent- und Markenrechte prüfen (siehe cad-project-license-check).
- Beschriftungslesbarkeit: Die Schrift auf den Flächen ist klein (etwa 7,5 pt) und weich, weil die Netzpunkte nicht auf Pixel gerundet sind. Vorschlag, falls es am Bildschirm zu klein ist: `LABEL_MIN_FACING` von 0,3 auf 0,5 und eine Mindesthöhe der Kappe von etwa 5 px, sonst `LABEL_FILL` 0,95. Der Nutzer entscheidet am Bildschirm. **Geändert in Etappe D (D5):** Schwellen 0,5/0,7, Füllung 0,95, Mindestschrift 9 pt, Würfel 100 px.
- Hover-Farbe der aktuellen Ansicht: behoben, der Akzent wird beim Überfahren abgedunkelt (`darken`); der Kontrast steht im Test `colours_in_both_themes`.
- Kanten und Ecken sind im Dunkeln kaum vom Viewport zu unterscheiden; einige Kommentare im Code zum hellen Thema stimmen nicht.
- Eckennamen wie „Oben-Vorne-Rechts“ sind in 10 px etwas breiter als der Streifen (damals 84 px) und ragen leicht über (bleiben in der freien Fläche). (Etappe D: der Streifen ist jetzt 100 px breit; ob das reicht, zeigt die Handprüfung.)
- Beim Wachsen des Schrift-Atlas kann die Beschriftung ein Bild lang falsch aussehen (UV wird beim Bauen normalisiert); die Beschriftung rechnet pro Bild 36 Galleys (Cache denkbar). **Erledigt in Etappe D:** Das Wachsen des Atlas verwirft das Bild; die Schriftgröße wird je Bildschirmmaßstab gemerkt.
- Ein Aufruf mit NaN-Kamera liefert 26 NaN-Stücke (die App malt nur endliche); ungültige `CubeRegion` (zum Beispiel `Edge{Top, Bottom}`) lassen sich bauen. **Erledigt in Etappe D:** NaN-Kamera und ungültige Bereiche werden abgefangen (`CubeRegion::is_valid`).
- `cube_faces` und `cube_hit_test` sind nur noch Test-Schnittstelle. **Erledigt in Etappe D:** nur noch `#[cfg(test)]` und privat.
- Tooltips des Würfels und die Facing-Schwelle 0,3 sind nicht getestet.

*Zeichnung*
- Das Werkzeugfeld füllt immer die volle Breite zwischen den Spalten; ab etwa 1400 px ist es rechts leer. Bei 800 × 600 ist es 244 px breit, beide Zeilen rollen seitlich (Balken sichtbar). Ob ein senkrechtes Mausrad die waagerechte Rollfläche bewegt, hängt von egui ab und ist nicht getestet. **Erledigt in Etappe D (D7):** Das Werkzeugfeld ist so breit wie sein Inhalt und mittig.
- Die Zeilengruppierung des Werkzeugfelds weicht vom Brief ab (siehe oben); der Nutzer entscheidet.
- Eine Meldung bei gefülltem Notizanteil bekommt nur etwa ein Viertel der Zeile.
- Test (g) der Zeichnungsfelder deckt den Primär-Zug nicht ab.
- Bei Zoom über 1 kann Papier unter einem Feld liegen; „Blatt einpassen“ legt es immer in die freie Fläche.

*Skizze*
- Der lange Hinweis „Skizzen-Modus: … Esc bricht ab …“ steht doppelt (im Statusfeld und im Fuß); bei 800 × 600 hat das Statusfeld drei Zeilen und kürzt. **Erledigt in Etappe D (D6):** Der Status ist kurz, der Tastenhinweis steht beim Überfahren.
- Eine nie gelöste Skizze zeigt „Vollständig bestimmt“ grau statt grün (wie vorher).
- Wird die Skizze mitten im Bild geöffnet (zum Beispiel per Doppelklick im Baum), rechnet `sketch_panel::show` ein Bild lang ein eigenes Layout; das linke Modell-Feld kann dabei ein Bild lang noch gezeichnet sein.
- Das Barrierefreiheits-Label des zweizeiligen Schalters enthält den Zeilenumbruch.
- Modell-Feld-Randtest ohne Mittelklick-Fall; der Fehlerfall-Test `want_text ""` ist trivial.
- Ein Klick auf den Würfel im Skizzenmodus kippt die Kamera weiterhin (bekannt aus Teilprojekt 4); Würfel und Home sind im Skizzenmodus getestet (fügen keine Punkte hinzu).

*Allgemein*
- Ein-Frame-Modus-Wechsel: Beim Wechsel zwischen Modell, Zeichnung und Skizze zeichnen die Felder des neuen Modus ein Bild lang mit vorläufigem Layout (neue Felder verwerfen ihren ersten Größen-Durchgang). **Erledigt in Etappe D (D8):** Der Moduswechsel verwirft das Bild.
- Die Tests treiben das echte `eframe::App::ui` und sehen kein echtes Rendering. Das Aussehen (Würfel, Leiste, Felder, Schriftgrößen) ist deshalb nur durch die Handliste abgedeckt. Die Zeitabhängigkeit (das erste egui-Bild ist langsam) bleibt ein Risiko für Tests mit Arbeitsthreads.
- Fase-Test: Der Menü- und Befehlsfeld-Test prüft zusätzlich per Quelltext-Suche, dass kein `Blend`-Literal mehr vorkommt; das ist brüchig und sollte über eine gemeinsame Funktion laufen. Veraltete Doc-Kommentare in `icons.rs` (um Zeile 730) und bei `paint_blend_icon`; Farbassertion im Chamfer-Test und ein Rest-Kommentar bei `painted_paths`.
- `layout()` ignoriert weiter `ShellState` (Seitenspalten immer vorhanden).
- Offene Punkte aus Etappe B zu kleinen Fenstern (unter 640 px Breite) gelten weiter.

**Handliste für den Nutzer** (`cargo run -p planora-app --release`; jeweils in Hell und Dunkel, in den Fenstergrößen 800 × 600, 1100 × 700 und einem großen Fenster)
1. Fenstergrößen und Themen: Felder bleiben im Fenster, überlappen einander nicht, nichts ist abgeschnitten, nichts flackert beim Größenändern. Hell/Dunkel je Dokument-Tab umschalten.
2. Würfel, Flächen: jede der 6 Flächen anklicken; die Ansicht stimmt, die Fläche ist blau, der Name steht unter dem Würfel.
3. Würfel, Kanten und Ecken: einige Kanten und Ecken anklicken (auch die verdeckten Bereiche über mehrere Klicks erreichen); Home: Isometrisch, alles im Bild.
4. Hervorhebung: nach Klick auf einen Bereich ist er blau; beim freien Drehen mit der Maus verschwindet die Hervorhebung, und der Name unter dem Würfel verschwindet auch; beim Ausrichten nahe an einem Bereich (innerhalb 2°) kommt sie wieder.
5. Beschriftung: Der Text auf der Fläche dreht sich mit, steht nie spiegelverkehrt und ist lesbar (Größe ok?). Hover über den Bereichen: Farbe gut, Tooltip „Ansicht: …“, auch auf der blauen Fläche.
6. Home-Knopf und Würfel: Klick und Ziehen auf dem Würfel drehen das Modell nicht; Mausrad über dem Würfel zoomt.
7. Zeichnung, Werkzeugfeld: alle Texte lesbar und vollständig, auch bei 800 × 600 (seitlich rollen); Umschalter 3D-Modell/Zeichnung, Ansicht hinzufügen, Blatt einpassen, Zoom − und +, Maße vorschlagen, Maß mit Modus.
8. Zeichnung, Ansichtsfeld: Ansicht wählen, Maßstab ändern, Verdeckte Kanten, Mittellinien (in der Isometrie gesperrt), Ansicht löschen; ohne gewählte Ansicht der Hinweis „Eine Ansicht auswählen.“
9. Zeichnung, Hinweisfeld und Blatt: Hinweise zum Werkzeug, Maßhinweise, Meldungen; das weiße Blatt füllt das Fenster und läuft unter die Felder; „Blatt einpassen“ und Zoom zentrieren in der freien Fläche; Maßwerkzeug, Maßvorschlag; Ansicht anklicken und ziehen.
10. Zeichnung, schreibgeschützt (alte Version öffnen): Hinweis oben, Werkzeugfeld darunter, Maße nicht änderbar.
11. Skizze, Leiste und Feld: Skizze öffnen (Erzeugen → Skizze auf XY-Ebene); alle 9 Werkzeuge anklicken (Name im Tooltip), Abhängigkeiten setzen, Liste mit Symbolknopf zum Löschen, „anzeigend“.
12. Skizze, Maßeingabe, Spiegeln und Muster: Maß eingeben (das Zahlenfeld hat Fokus, Esc/Enter/Entf wirken nicht auf die Skizze), OK und Abbrechen darunter; Spiegeln und Muster benutzen; der Schalter „Waagerecht/senkrecht erkennen“ (zweizeilig, Beschriftung ok?).
13. Skizze, Fuß: Status in Farbe (Warnung, Fehler), Werkzeughinweis, „Skizze beenden“ (die Ansicht kommt zurück); der lange Hinweis doppelt?
14. Eingabe über Feldern: Klick, Ziehen und Mausrad auf Feldern (auch ganz am Rand, innen) bewegen weder Modell noch Blatt noch Skizze; ein im Bereich begonnenes Ziehen läuft über Felder weiter.
15. Dialoge über Feldern: Export-, Rundungs-, Boolescher Dialog und Rückfragen liegen über allen Feldern, in allen drei Modi.
16. Fase-Symbol: im Menü „Bearbeiten“, im Befehlsfeld und im Baum (gerade Schräge, vom Verrunden unterscheidbar).
Beanstandungen sammeln; sie werden nachgebessert oder in Etappe D behoben.

**Zahlen (Stand 2026-10-16, Gesamtreihe)**
- `cargo test --workspace -j 2`: 2220 bestanden, 0 fehlgeschlagen, 33 ignoriert, rund 15 Minuten (Debug), kein sporadischer Fehler.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` und `cargo deny check` sind grün; die geschützten Pfade (Kernel, Netz, STEP, Austausch) sind gegenüber 945fa67 unverändert.

**Handprüfung des Nutzers (Haltepunkt 3, 2026-10-17)**
- Befund 1, Würfel: „komische Linien, die beim Bewegen der Kamera aufblitzen“. Ursache (per Test nachgestellt): Bereiche, die fast flach zur Kamera stehen, wurden schon ab einer Zuwendung von 0,02 gemalt, also als 0,3–1,5 px schmale Streifen mit 1-px-Umriss am Würfelrand; beim Drehen tauchten sie schlagartig auf und verschwanden wieder (Prüflauf alle 3° Gier/Neigung von −85° bis 85°: 5700 solche Streifen; Prüflauf in 1°-Schritten: 468 Sprünge von unsichtbar auf voll deckend, auch bei der Beschriftung an der Schwelle 0,3). Behoben: gemalt wird ein Bereich erst ab 0,15 (`viewport::PIECE_MIN_FACING`, der schmalste ist dann etwa 1,7 px breit), Füllung und Umriss blenden bis 0,3 weich ein (`PIECE_FULL_FACING`), die Beschriftung von 0,3 bis 0,5 (`LABEL_FULL_FACING`). Hover und Klick nutzen dieselbe Liste (`clickable_pieces`): anklickbar ist nur, was mindestens halb deckend zu sehen ist. Neu in `planora-render`: `cube_pieces_visible(camera, min_facing)`; `cube_pieces` bleibt bei 0,02. In allen Würfelansichten (Flächen, Kanten, Ecken, Isometrie) sind alle sichtbaren Bereiche voll deckend, Farben und Hervorhebung dort unverändert. Tests: `tests/view_cube_artefacts.rs`.
- Befund 2, Skizze auf einer gewählten Fläche: erschien klein und nicht mittig. Ursache: Eine leere Skizze setzte nur das Ziel auf den Ursprung der Skizzenebene (bei einer Fläche oft eine Ecke) und behielt den Abstand; eine Skizze mit Geometrie wurde über die Kugel um die Hülle eingepasst (füllte nur etwa 48 %), und zwar mit dem freien Rechteck des Modellmodus. Behoben: `open_sketch` blickt senkrecht auf die Ebene und passt mit `OrbitCamera::frame_points` (neu, ebene Einpassung mit 15 % Rand) ein: leere Skizze auf einer Fläche → der Umriss der Fläche (Punkte des Anzeigenetzes, in die Ebene gelegt); leere Skizze auf einer Grundebene (oder Fläche nicht mehr auffindbar) → die Hüllen aller Körper, in die Ebene gelegt, ohne Körper ±25 mm um den Ursprung der Ebene; Skizze mit Geometrie → ihre Hülle (mindestens 10 mm). Die 3D-Ansicht passt in den folgenden Bildern mit dem freien Rechteck des Skizzenmodus nach (`AppState::refit_sketch_view`), bis es drei Bilder gleich bleibt oder der Nutzer verschiebt, zoomt oder den Würfel benutzt. Tests: `tests/sketch_framing.rs`.
- Nicht am Bildschirm geprüft (keine Sicht auf die laufende App): wie das Ein- und Ausblenden der Würfelbereiche aussieht, und ob die Skizzenansicht auch beim Öffnen per Doppelklick im Baum sofort richtig sitzt (der Weg ist derselbe; getestet ist der Ablauf wie beim Menü: Anlegen und Öffnen vor der 3D-Ansicht des nächsten Bildes, nicht der Klick auf den Menüeintrag selbst).

**Folgearbeit (Etappe D und Aufräumen)**
- Dialoge im neuen Rahmen und mittig schwebend (neue Breitenregel). **Erledigt in Etappe D**; Checkboxen und Kontextmenü-Symbole sind es nicht (siehe Etappe D, Grenzen).
- `pub`-Fläche aufräumen (Testhaken aus Etappe B und C, `app::{CreateKind, create_blocked, do_create, open_boolean, open_blend}`, `viewport::show`, `shell::layout`, `tabs::show`, `versions_panel::show`, `cube_faces`/`cube_hit_test`) und tote Konstanten.
- Hover-Kontrast der aktuellen Würfelansicht (abdunkeln) und Kontrast der Kanten/Ecken im Dunkeln; Beschriftungsgröße nach dem Handtest.
- Test-Aufräumen: Fase-Test ohne Quelltext-Suche, veraltete Doc-Kommentare, Tests der Zeichnungsfelder um den Primär-Zug und Mittelklick ergänzen.
- Nach dem Handtest: Vorschlag des Nutzers zum Raster der Hauptansicht besprechen.

## Teilprojekt 13 – Graphit-Oberfläche, Etappe D (Dialoge, Abschluss)
Stand: 2026-10-17. Alle Dialoge und Rückfragen haben den neuen Rahmen, der Würfel ist aufgeräumt, und die Reste aus Etappe C sind erledigt. Das ist Etappe D von vieren und der Abschluss von Teilprojekt 13. Spec: `specs/2026-10-16-graphit-oberflaeche-design.md` (Abschnitt 13 „Festlegungen der Umsetzung Etappe D“). Plan: `plans/2026-10-17-graphit-etappe-d.md`. Zweig `feature/graphit-etappe-d`.

**Was geliefert wurde**
- Neues Modul `dialog.rs` (`Dialog`, `DialogSize`): jedes Dialogfenster entsteht an dieser einen Stelle. Mitte des Fensters, feste Breite je Klasse (Rückfrage 440, Normal 480, Breit 720 px), Graphit-Rahmen, eigene Kopfzeile mit Titel und Schließen-Symbol, verworfener erster Größen-Durchgang, Höhe nach Inhalt, senkrechtes Rollen nur bei Inhalt höher als das Fenster (die Kopfzeile rollt nicht mit), Begrenzung auf das Fenster abzüglich 16 px Rand.
- Alle Dialoge nutzen den Rahmen: Körper anlegen, Extrudieren/Drehen, Boolesche Operation, Feature bearbeiten, Rundung/Fase (Normal); Exportieren, Netz importieren, Neue Zeichnung, Maß bearbeiten (Breit); Umbenennen und Löschen in Baum, Zeichnungen und Maßen, Projekte und Versionen, alle Rückfragen (Prompt). `theme::window`, `export_dialog::wide_window`, `DIALOG_TOP`, `DIALOG_MARGIN` und `MIN_WIDTH` sind entfernt. Der Wächtertest `no_raw_windows` verhindert neue rohe Fenster.
- Verschieben: Alle Dialoge außer den festen Rückfragen (Projekte, Versionen, Änderungen speichern und so weiter) lassen sich an ihrer freien Fläche und am Titel verschieben und bleiben im Fenster. Ein Schließen-Symbol haben genau die Dialoge, die bisher das Kreuz hatten; dazu gehört die „Rückfrage“ beim Löschen in Baum, Maß- und Zeichnungsliste (verschiebbar, mit Kreuz).
- Karten nebeneinander bleiben gleich hoch, auch in 720 px und bei 800 × 600.
- Würfel (`planora-render`, `planora-app/viewport.rs`): Interna sind nicht mehr öffentlich (`cube_faces`, `cube_hit_test`, `CubeFace` nur noch im Test); `CubeRegion::is_valid`; NaN-Kamera und ungültige Bereiche werden abgefangen; Würfel 100 px; Beschriftung ab Zuwendung 0,5, voll ab 0,7, Füllung 0,95, Mindestschrift 9 pt, Schriftgröße je Bildschirmmaßstab gemerkt; wächst der Schrift-Atlas, wird das Bild verworfen.
- Ziehen auf dem Würfel dreht die Ansicht (Modell und Skizze); Umschalt + Ziehen dreht nicht (dort verschiebt die Ansicht). Ein Klick ohne Ziehen wählt weiter den Bereich; das Mausrad über dem Würfel zoomt.
- Skizzenstatus kurz („Skizzen-Modus: Name“); der Tastenhinweis erscheint beim Überfahren des Statusfelds und steht in der Beschriftung für Bildschirmleser.
- Werkzeugfeld der Zeichnung so breit wie sein Inhalt und mittig (`shell::fit_tool`).
- Moduswechsel (Modell, Zeichnung, Skizze) ohne Zwischenbild: Das Bild wird verworfen und wiederholt.
- Enter übernimmt jetzt auch in Neuer Ordner, Projekte umbenennen und Kommentar ändern (vorher nur im Baum und bei Zeichnungen).

**Entscheidungen D1–D8** (Wortlaut in der Spec, Abschnitt 13; D5 mit Entscheidung des Nutzers)
- D1 (Nutzer) Alle Dialoge schmaler und mittig; breite bis 720 px; Rückfragen mittig und schmal.
- D2 Feste Breiten je Klasse: Prompt 440, Normal 480, Wide 720, höchstens Fensterbreite − 32.
- D3 Mitte des Fensters; verschiebbar außer festen Rückfragen (wegen der Kantenwahl bei Rundung/Fase).
- D4 Eigene Kopfzeile mit Schließen-Symbol statt egui-Titelleiste.
- D5 (Nutzer) Würfel 100 px (ändert C1), Beschriftungsschwellen 0,5/0,7/0,95, Mindestschrift 9 pt.
- D6 Skizzenstatus kurz, Tastenhinweis beim Überfahren.
- D7 Werkzeugfeld der Zeichnung nach Inhalt.
- D8 Moduswechsel verwirft das Bild. Wurde nicht gestrichen: Ohne diesen Schritt blitzt das Feld des alten Modus ein Bild lang auf.

**Entscheidungen der Umsetzung (Rulings)**
- Die Grenzen des egui-Fensters gelten für die Außengröße (mit Rahmen); `show()` übergibt die Klassenbreite deshalb unverändert.
- Eigene Schließbehandlung statt des egui-Kreuzes; `Dialog` liefert nichts mehr, wenn `*open` falsch ist. Das Schließen-Symbol meldet sich zusätzlich als „Schließen“ für Bildschirmleser (zweite `widget_info`, im Code erklärt).
- Breite Dialoge: Die Höhe bleibt höchstens 568 px bei 800 × 600. „Exportieren …“ mit Dateiwahl war dort nur durch Rollen erreichbar; seit die Höhe dem Inhalt folgt (etwa 475 px), passt er bei 800 × 600 und 1400 × 900 ganz und rollt nicht.
- Höhe nach Inhalt (Abschluss-Nachbesserung): Der Rollbereich ist ein eigener unter der Kopfzeile (nicht der des egui-Fensters, der jeden Dialog auf 420 px streckte). Die Kopfzeile hat eine feste Zeilenhöhe, sonst misst egui sie im ersten Durchgang niedriger und der Dialog stünde ein Bild lang versetzt. Ein wieder geöffneter Dialog verwirft sein erstes Bild, damit er mit der neuen Höhe mittig steht; ebenso ein offener Dialog, dessen Größe sich ändert (zum Beispiel Exportieren beim Öffnen der Dateiwahl), sonst stünde er ein Bild lang versetzt. Der Titel ist nicht markierbar, damit Ziehen am Titel den Dialog verschiebt.
- Die Statuszeile ist ein Tab-Halt für die Bedienhilfe; mit Fokus zeigt sie den Fokusrahmen, und Esc, Entf und Eingabe wirken weiter auf Skizze und Ansicht (`viewport::focus_is_not_typing`). Dasselbe gilt für den Fokus auf der Zeichenfläche der Skizze und auf der 3D-Ansicht. Würfel und Home-Knopf sind gewöhnliche Knöpfe: Haben sie den Fokus, sind die Tasten der Ansicht gesperrt (wie bei jedem fokussierten Knopf).
- Die Schwelle zum Ziehen am Würfel ist 4 px (egui hat 6); der Würfel prüft selbst gegen Druckpunkt und Zeiger. Während des Ziehens gibt es keine Hervorhebung und keinen Tooltip.
- Bei 9 pt ist das längste Wort (Montserrat Medium, Maßstab 1) 32 px breit, erlaubt wären 31,9 px. Die Mindestschrift hat Vorrang; der Test erlaubt dort 0,5 px Rundung.
- Das Statusfeld ist als Ganzes Hover-Ziel (nicht nur der Text) und hat die Beschriftung „Skizzen-Modus: Name. Hinweis“; egui meldet Beschriftungen als Wert, nicht als Label.
- Das Werkzeugfeld bei 1100 × 700 ist schmaler als vorher (Inhalt 520 px statt 544 verfügbar): Es ist immer so breit wie der Inhalt, höchstens so breit wie der Platz zwischen den Spalten. Bei 800 × 600 bleibt es voll breit und rollt. Die Breite ist gerade aufgerundet, damit das mittige Feld auf ganzen Bildpunkten sitzt; ebenso beim Fußfeld der Skizze (Toleranz im Test `near` bleibt < 0,5).
- Beim ersten Messen merkt sich das Werkzeugfeld seine Breite und verwirft das Bild einmal; ohne diesen Schritt wäre es ein Bild lang voll breit.
- Neue Testhaken (`doc(hidden)`): `PlanoraApp::panels_mut`, `projects_panel::request_*` und `dialog_open`, `shell_panels::drawing_tool_wanted`, `viewport::region_point`.
- Tests wurden zum Teil nach der Umsetzung geschrieben; die Mutationsbeweise (Fehler absichtlich einbauen, Test muss rot werden) stehen in den Aufgabenberichten unter `.superpowers/sdd/2026-10-17-graphit-etappe-d/`.
- Geschützte Pfade (Kernel, Netz-Quelltext, STEP, Austausch) unverändert gegenüber 945fa67; keine Änderung an Cargo-Abhängigkeiten.

**Tote Konstanten (Aufgabe 6, Schritt 1):** Alle 136 `pub const` und `pub(crate) const` in `crates/planora-app/src` geprüft (Fundstellen je Name per Suche): keine kommt nur in ihrer Definition vor, es musste nichts mehr entfernt werden. Schon vorher entfernt: `DIALOG_MARGIN`, `DIALOG_TOP`, `MIN_WIDTH` (Aufgabe 2) und `theme::window` (Aufgabe 3).

**Grenzen und offene Punkte**

*Dialoge*
- Dialoge schließen nicht mit Esc (wie bisher). Die Rückfragen lassen sich nur über ihre Knöpfe beenden.
- Seit die Höhe dem Inhalt folgt, steht ein fester Dialog nur dann schon im ersten Bild genau mittig, wenn egui seinen Inhalt im ersten, messenden Durchgang gleich hoch misst wie danach (für die Kopfzeile ist das gesichert; beobachtet wurde, dass egui eine waagerechte Reihe mit nur Text darin dort niedriger misst). Sonst rückt er im zweiten Bild um die halbe Differenz zurecht. Nicht für jeden Dialog einzeln getestet.
- Die Checkboxen in Dialogen und die Symbole im Kontextmenü sind nicht im Graphit-Stil überarbeitet (stand in der Folgearbeit von Etappe A); sie waren nicht Teil dieses Plans.
- Die Dialogtests prüfen vor allem Lage, Größe, Ebene und Bedienbarkeit; die Wirkung jedes einzelnen Dialogs prüfen sie nicht, nur bei Neuer Ordner und Kommentar ändern ist sie ergänzt. Der Wächter `no_raw_windows` ist eine Textsuche und deshalb nur eine Faustregel.

*Würfel*
- Die Beschriftung ist bei 9 pt lesbar, aber klein; ob sie am Bildschirm reicht, entscheidet die Handprüfung. Das längste Wort ist an der Mindestschrift etwa 0,1 px breiter, als die Fläche erlaubt.
- Der Zwischenspeicher der Schriftgröße richtet sich nach Bildschirmmaßstab und Atlasgröße, nicht nach einem späteren Schriftwechsel (Schriften werden zur Laufzeit nicht gewechselt).
- Wächst der Atlas mitten im Bild, wird das Bild verworfen und wiederholt (nicht am Bildschirm gesehen).
- Ein NaN-Ziel der Kamera ist nicht abgefangen; Gier, Neigung und Abstand sind es.
- Zum Würfelziehen sind nicht alle Sonderfälle getestet (zum Beispiel Loslassen außerhalb des Fensters); das Zurücksetzen des Merkers nach übersprungenen Bildern ist nur im ersten Bild beobachtbar (egui-Trefferliste).
- Kanten und Ecken im Dunkeln und die Tooltips sind weiter nicht gesondert getestet (Handprüfung).
- Das Patent- und Markenrecht des Würfels ist vor einer Veröffentlichung zu prüfen (siehe Etappe C).

*Zeichnung und Skizze*
- Bei 1100 × 700 ist das Werkzeugfeld schmaler als in Etappe C; das ist gewollt (D7), der Nutzer kann es am Bildschirm beanstanden.
- Ob ein senkrechtes Mausrad das seitlich rollende Werkzeugfeld bewegt, hängt von egui ab und ist nicht getestet.
- Die Zeilengruppierung des Werkzeugfelds weicht weiter vom ersten Brief ab (Etappe C).
- Der Tastenhinweis der Skizze steht nur beim Überfahren und in der Beschriftung des Statusfelds, nicht dauerhaft sichtbar.

*Allgemein*
- Die Tests treiben das echte `eframe::App::ui`, sehen aber kein echtes Rendering. Das Aussehen (Rahmen, Schließen-Symbol, Schriftgrößen, Verschieben) ist nur durch die Handliste abgedeckt.
- Verbleibende öffentliche Fläche (nicht aufgeräumt, weil nicht Teil dieses Plans): `app::{CreateKind, create_blocked, do_create, open_boolean, open_blend}`, `viewport::show`, `shell::layout`, `tabs::show`, `versions_panel::show`, die Module `theme` und `icons` sowie die Testhaken in `shell_panels` und `PlanoraApp` (jetzt mit den neuen aus der Liste oben). Offene Frage an den Nutzer: aufräumen (eigene Aufgabe) oder so lassen.
- Fase-Test ohne Quelltextsuche: nicht umgesetzt (Etappe C, Aufräumen).
- Offene Punkte aus Etappe B zu Fensterbreiten unter 640 px gelten weiter.
- Die Zeitabhängigkeit der Tests mit Arbeitsthreads (das erste egui-Bild ist langsam) bleibt ein Risiko.

**Handliste für den Nutzer** (`cargo run -p planora-app --release`; jeweils in Hell und Dunkel, in den Fenstergrößen 800 × 600, 1100 × 700 und einem großen Fenster)
1. Alle Dialoge öffnen und prüfen (mittig, Breite, nichts abgeschnitten, nichts flackert): Körper anlegen (Quader, Zylinder, Kegel, Kugel, Torus), Extrudieren, Drehen, Boolesche Operation, Feature bearbeiten, Rundung, Fase.
2. Exportieren in allen Formaten, Netz importieren, Neue Zeichnung, Maß bearbeiten.
3. Umbenennen und Löschen im Baum, bei Zeichnungen und bei Maßen; im Projektfeld Neuer Ordner, Umbenennen, Verschieben, Löschen; bei Versionen Kommentar ändern und Löschen.
4. Rückfragen: Änderungen speichern, Kommentar, Speichern unter, Sperre übernehmen, Autosicherung, Hinweis. Sie liegen mittig, sind schmal und lassen sich nicht verschieben.
5. Karten (Exportieren, Netz importieren, Neue Zeichnung) stehen nebeneinander und sind gleich hoch.
6. Texte sind nicht abgeschnitten; Exportieren mit Dateiwahl passt bei 800 × 600 ganz ins Fenster, der Aktionsknopf ist ohne Rollen erreichbar. Nur ein Dialog, der höher als das Fenster ist, rollt (Titel und Kreuz bleiben oben).
7. Schließen-Symbol oben rechts schließt den Dialog; der Fokusring ist mit der Tabulatortaste sichtbar.
8. Dialog verschieben: an der freien Fläche oder am Titel ziehen; er bleibt im Fenster. Kurze Rückfragen sind niedrig (keine leere Fläche unter den Knöpfen).
9. Rundung: Dialog öffnen, Kanten im Modell neben dem Dialog anklicken (oder den Dialog beiseite ziehen).
10. Enter in Neuer Ordner, Umbenennen und Kommentar ändern übernimmt.
11. Würfel: Beschriftung lesbar und nicht spiegelverkehrt; Ziehen auf dem Würfel dreht, Umschalt + Ziehen dreht nicht, Klick wählt den Bereich, Mausrad über dem Würfel zoomt; keine Hervorhebung während des Ziehens.
12. Skizzenstatus ist kurz („Skizzen-Modus: Name“); der Tastenhinweis erscheint beim Überfahren des Statusfelds.
13. Zeichnung im großen Fenster: Werkzeugfeld schmal und mittig; bei 800 × 600 volle Breite und rollbar.
14. Moduswechsel (Modell, Zeichnung, Skizze, auch per Doppelklick im Baum): Es blitzen keine Felder des alten Modus auf.
Beanstandungen sammeln; sie werden in einer Korrekturrunde behoben.

**Zahlen (Stand 2026-10-17)**
- `cargo test --workspace -j 2 --no-fail-fast` (Debug): 2294 bestanden, 0 fehlgeschlagen, 33 ignoriert in 255 Testprogrammen (Etappe C: 2220 bestanden, 33 ignoriert).
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` und `cargo deny check` (advisories, bans, licenses, sources ok) sind grün; die geschützten Pfade (Kernel, Netz-Quelltext, STEP, Austausch) sind gegenüber 945fa67 unverändert.
- Umfang der Etappe gegenüber dem Plan-Stand (ef00f60 bis 314d2cf): 51 Dateien, +4417/−975 Zeilen.

## Aufräumen / Qualität
- `rust-version = "1.88"` ist für die GUI-Crates ungenau (egui 0.36 braucht 1.95).
- `eframe` mit Standard-Features baut auch den `glow`-Renderer mit.
- CI baut `cargo-deny` bei jedem Lauf neu.
- Tests: Pan-/Orbit-Vorzeichen, Auswahl vor Hover, Vertex-Größen, `UnsupportedSegment`, Torus bei Uhrzeigersinn-Kette.
- Auswahl: CPU-Raycast läuft bei jeder Mausbewegung über alle Dreiecke (später Hüllkörper/Raumindex).
- Hover-Änderung baut die ganze Szene neu und lädt sie hoch (später nur Farben aktualisieren).
- Browser: lange Namen werden abgeschnitten; Klick im Browser setzt keine Flächenauswahl.

## Teilprojekt 3c-1 – Zeichnungsblatt

### Normprüfung
Die Normtexte (ISO 5457, 7200, 5456-2, 3098) sind kostenpflichtig; geprüft wurden nur öffentlich auffindbare Zusammenfassungen (Stand 2026-10-17). Die Zahlen der Spec blieben unverändert. „offen“ heißt: öffentlich nicht bestätigt, nicht widerlegt.

| Spec-Wert | gefundener Wert | Quelle | Abweichung |
|---|---|---|---|
| Rand links 20 mm, sonst 10 mm | 20 mm Heftrand, 10 mm sonst | Sekundärquelle (Blog), Normtext nicht gesehen | nein (unbestätigt) |
| Rahmen 0,7 mm | 0,7 mm | Auszug ISO 5457:1999 (Suchergebnis) | nein |
| Zentriermarken 0,5 mm | Auszug nennt 0,7 mm für Zentriermarken | Auszug ISO 5457:1999 (Suchergebnis) | zu klären, Entwurfswerte unverändert |
| Zentriermarken reichen 5 mm hinter den Rahmen | Auszug: vom Blattrand bis 10 mm hinter den Rahmen | Auszug ISO 5457:1999 (Suchergebnis) | zu klären, Entwurfswerte unverändert |
| Rasterfeld 50 mm, Felderzahl 2·round(Kante/100), Buchstaben ohne I/O | nicht auffindbar | – | offen |
| Teilstriche 5 mm, 0,25 mm, Beschriftung mittig im Rand (3,5 hoch) | nicht auffindbar; Spec-Wort „Randteilung am Rahmen innen“ mehrdeutig, Teilstriche werden vom Rahmen nach außen gezeichnet | – | offen |
| Schriftfeld 180 mm breit, Feldhöhen 2,2 / 3,5 / 5 | nicht auffindbar | – | offen |
| Projektionssymbol (F7): Maße, Lage je Methode | nicht auffindbar; Quellen widersprechen sich bei der Lage der Kreise | Suchergebnisse | offen |
| Schriftreihe 2,5 / 3,5 / 5 (Spec nennt 2,2 und 3,3) | nicht auffindbar | – | offen |


Fehler und Korrektur (Aufgabe 1): Die rechte Zentriermarke war zunächst 25 mm lang (A4 quer `(297,105)–(272,105)`, 15 mm hinter dem Rahmen bei 287), weil der Heftrand statt des 10-mm-Rands verwendet wurde; der Wert stammte aus einem Fehler der Aufgabenvorgabe. Richtig nach Spec: Marken laufen vom Blattrand bis 5 mm hinter den Rahmen, rechts also `297 → 282`. Behoben; oben (15 mm vom Rand) war richtig.

### Stand und Geliefertes
Stand: 2026-10-17 (Zweig `feature/zeichnungsblatt`). Aus dem Blatt mit dünnem Innenrahmen ist ein formal vollständiges Zeichnungsblatt geworden. Bedienung und Grenzen für Anwender: README, Abschnitt „Zeichnungsblatt“. Festlegungen F1 bis F15 und Abweichungen: Spec §10.

- `planora-drawing`: Blattgeometrie nach Konstanten (`sheet_frame.rs`: Rahmen mit Heftrand, Zentriermarken, Raster, Schriftfeld mit 17 Zellen, Änderungstabelle, Hinweisfeld, freier Bereich), Angaben mit Vorbelegung und Auflösung zu Texten (`sheet_info.rs`: automatisch oder eigener Text, Änderungsbuchstaben, Hauptmaßstab, Blatt n/N, Grenzen), Blatt als Markierungen mit Einpassen der Texte (`sheet_marks.rs`, neue `MarkKind::Sheet(SheetPen)`), SVG-Gruppe `Blattrahmen`, DXF-Ebenen `BLATTRAHMEN`, `BLATT_MITTE`, `BLATT`; der alte Innenrahmen entfällt aus der Ausgabe.
- `planora-feature`: projektweite Angaben und Blattangaben im Dokument (`set_project_info`, `set_sheet_info`, `add_drawing_stamped`), Prüfung beim Ändern und Laden.
- `planora-project`: Dateiformat 6 (nur mit Projekt- oder Blattangaben; Format 1 bis 5 laden und speichern Byte für Byte wie bisher, eingefrorene Prüfdateien für Format 4 und 5).
- `planora-app`: Blatt in der Zeichnungsansicht (weißes Papier in Hell und Dunkel), neue Ansichten im freien Bereich (Dreiansichtenblock mittig, Isometrie darin), Fenster „Blattangaben“ (720 px, drei Abschnitte, Hinweis, Zurücksetzen, Länge, Warnung bei Kürzung), Knopf im Werkzeugfeld und Menü „Zeichnung“, Schalter „Blattrahmen mit ausgeben“ im Export.
- Proben: `cargo test --release -p planora-drawing --test hand_samples_sheet -- --ignored` schreibt 80 Dateien nach `D:\Testumgebung\zeichnung-proben\blatt` (abweichend über `PLANORA_SHEET_SAMPLES_DIR`): A4 hoch, A4 quer, A3 quer, A2 hoch, A0 quer; Methode 1 und 3; „kurz“ (Vorgaben) und „lang“ (Längstexte, Sonderzeichen, 23 Änderungen, 6 Hinweiszeilen); je SVG und DXF mit und ohne Rahmen.
- Prüfstand 2026-10-17 (nach der Abschlussrunde mit Skalierung): `cargo test --workspace -j 2`: 2387 Tests grün, 34 ausgeschaltet, 0 Fehler (vorher 2374; nach den Funden des Handtests); `cargo fmt --check` und `cargo clippy -D warnings` sauber; `cargo deny check`: Lizenzen, Quellen und Verbote in Ordnung, die Advisories melden drei neue „unmaintained“-Einträge zu `zbus_names`, `zvariant`, `zvariant_derive` (RUSTSEC-2026-0371/0370/0372; über das Fenstersystem hereingeholt, `Cargo.lock` seit dem Basisstand unverändert, nicht Teil von 3c-1); diese drei sind seit 1bbc655 mit Wiedervorlage ignoriert, `cargo deny check` ist damit grün (advisories, bans, licenses, sources ok).

### Festlegungen und Abweichungen (Kurzfassung, Einzelheiten Spec §10)
- **Skalierung mit dem Format (Entscheidung des Nutzers, 2026-10-17):** Rahmen, Raster, Schriftfeld, Tabellen, Hinweisfeld, Schrifthöhen und Projektionssymbol sind die A3-Werte mal `lange Blattseite / 420` (A4 0,707, A2 1,414, A1 2,002, A0 2,831), hoch und quer gleich. Jedes Format sieht aus wie ein verkleinertes oder vergrößertes A3-Blatt; das Raster hat überall 8 × 6 Felder (hoch 6 × 8), die Hinweisfeld-Regel gilt mit 310 · Faktor (A4 quer: daneben). Linien wachsen mit, nie unter 0,18 mm (A4 0,49 / 0,35 / 0,18; DXF rundet auf die nächste zulässige Stärke, A4 50 / 35 / 18). **Abweichung von der Praxis:** üblich ist ein festes 180-mm-Schriftfeld mit 2,5 / 3,5 / 5 mm Schrift auf allen Formaten; hier ist es auf A4 127 mm breit, Inhalte 2,47 mm und Feldnamen etwa 1,6 mm hoch, auf A0 alles 2,8-mal so groß.
- **Isometrie im leeren Viertel:** Die isometrische Ansicht kommt in das leere Viertel des Dreiansichtenblocks (Erstwinkel rechts unten, Drittwinkel links oben) und bekommt, wenn sie dort sonst nicht hineinpasst, die nächstkleinere Stufe der Maßstabsreihe (höchstens eine Stufe kleiner als die Grundansicht, damit sie nicht winzig wird). Passt sie auch so nicht (fast blattfüllende Teile, oder die Grundansicht wurde von Hand so gelegt, dass das Viertel schmal ist), sitzt sie im Maßstab der Grundansicht bündig in der Ecke des freien Bereichs und kann den Block überdecken.
- **DXF-Texte mit Breitenzuschlag:** In DXF ist die Texthöhe die Großbuchstabenhöhe; CAD-Schriften laufen 1,1- bis 1,7-mal breiter als die Schätzung. Die Rahmentexte im DXF werden gegen die 1,45-fache Schätzung eingepasst (kleiner bis drei Fünftel, dann „…“); in DXF kann deshalb ein Text kleiner oder kürzer sein als im SVG. Die Warnung im Fenster richtet sich nach der SVG-Regel.
- **Grundansicht allein mittig:** Eine neue Zeichnung zeigt ihre Grundansicht in der Mitte des freien Bereichs. Mit der ersten weiteren Ansicht rückt sie (im selben Schritt), damit der Block aus drei Ansichten mittig liegt – nur, wenn sie noch an ihrer automatischen Stelle steht; eine verschobene Grundansicht bleibt.
- **Änderungstabelle mit 3 Zeilen (Entscheidung des Nutzers nach dem Handtest):** auf dem Blatt die letzten 3 Änderungen (vorher 6); die Kürzungswarnung nur für diese. Die automatische Anordnung und der Maßstabsvorschlag rechnen immer mit Kopf und 3 Zeilen, damit eine später gefüllte Tabelle keine Ansicht überdeckt (vorher: mit der aktuellen Zeilenzahl, im Handtest überdeckte die volle Tabelle die Ansicht „Links“).
- **Maßstabsfeld:** zeigt den Maßstab der ersten Ansicht der Zeichnung (Grundansicht); vorher den der Ansicht mit den meisten Linien, was im Handtest die Isometrie (1:1) statt der Hauptansichten (2:1) ergab. Ohne Ansichten steht „–“.
- **Zelle „Index“:** Die Zelle des Änderungsstands heißt im Schriftfeld „Index“; im Fenster heißt sie weiter „Änderungsstand“.
- Zentriermarken bleiben bei 0,5 mm Stärke und 5 mm hinter dem Rahmen (Spec), obwohl ein öffentlicher Auszug von ISO 5457:1999 0,7 mm und 10 mm nennt; ungeprüft, siehe Normtabelle oben. Die Zahl 25 mm für die rechte Marke in der Aufgabenvorgabe war ein Fehler und ist berichtigt (282 statt 272 bei A4 quer).
- Das Projektionssymbol liegt in der DXF-Datei, weil die Ebene nach Strichstärke gewählt wird, auf `BLATT_MITTE` zusammen mit den Zentriermarken, nicht auf `BLATT`.
- Die Textbreite (F11) ist eine vorsichtige Schätzung; breite Zeichen (CJK u. a.) werden unterschätzt.
- Die isometrische Ansicht ist bei Teilen, die fast das ganze Blatt füllen, „bestmöglich“: sie kann den freien Bereich überragen oder den Block überdecken.
- Alte Zeichnungen (vor dem Schriftfeld) können mit Schriftfeld oder Tabelle zusammenstoßen; Ansichten werden nicht automatisch verschoben.
- Blatttexte unter einem Bildpunkt (A0 bei 800 × 600) werden auf einen Bildpunkt angehoben statt weggelassen (nur Blatt-Markierungen).
- Der Knopf „Blattangaben“ steht am Ende der ersten Zeile des Werkzeugfelds, nicht „neben Blatt einpassen“ (Spec §5): sonst rollt das Werkzeugfeld schon bei 1100 × 700.
- Ein leeres Feld bedeutet „automatisch“ (F1); ein Feld lässt sich nicht auf „leer“ erzwingen (auch Prüfer und Freigabe nicht).
- Kürzungswarnung je Zeile der Änderungstabelle (eine Meldung, auch wenn mehrere Zellen betroffen sind) und je Hinweiszeile.
- Die Vorgabetexte „ISO 2768-m“ (Allgemeintoleranz) sind Blattinhalt (formaler Zeichnungstext) und keine technischen Hinweise der Oberfläche.
- Eine Zeichnung mit Blattangaben macht das Dokument zum Dateiformat 6; ältere Programme lehnen es mit benannter Meldung ab. Ein Dokument mit nur Projektangaben bleibt nach dem Löschen der letzten Zeichnung im Format 6; ein Format-6-Dokument ohne beide Datensätze lädt nachsichtig.
- DXF: Ein Blatttext, der in der Datei länger als 255 Zeichen würde, sperrt den DXF-Export schon vor dem Schreiben mit „Der Text im Feld „…“ ist für DXF zu lang. Bitte kürzen.“ (kein stilles Kürzen); ein Zeichen außerhalb des DXF-Zeichensatzes sperrt nur den DXF-Export mit „Das Zeichen U+… im Feld „…“ lässt sich als DXF nicht ausgeben. Bitte ändern.“ (Zeichennummer statt des Zeichens, das die Schrift oft nur als leeres Kästchen zeigt).

### Grenzen (zusätzlich zu den Punkten oben)
- **Datum in UTC (F2):** kurz nach Mitternacht Ortszeit kann das Ausgabedatum der Vortag sein; änderbar.
- **Geschätzte Textbreite (F11):** echte Schriften (Inkscape, Browser, andere Schriftart als die Standardschrift) können breiter laufen; konservativ gewählt, von Hand prüfen. Für DXF gilt der Zuschlag 1,45; eine CAD-Schrift, die noch breiter läuft (bis 1,7), kann knapp über ihre Zelle ragen.
- **Kleine Schrift auf A4:** Mit der Skalierung sind die Inhalte auf A4 2,47 mm hoch, Feldnamen etwa 1,6 mm, Rasterbeschriftung 2,47 mm; gedruckt lesbar, aber unter der üblichen Mindesthöhe von 2,5 mm (ISO 3098, ungeprüft). Eingekürzte Texte werden auf A4 bis 1,5 mm klein.
- **Fusion 360 zeigt DXF-Texte nicht** (bekannt aus Teilprojekt 12): Schriftfeld und Beschriftung sind dort nur als Linien zu sehen; LibreCAD oder QCAD zeigen die Texte.
- **Kein Platz für die Stückliste (3c-2) freigehalten:** 3c-2 muss den freien Bereich neu zuschneiden (erledigt in 3c-2: der Platz für 10 Zeilen wird freigehalten).
- **Keine Blattreihenfolge:** Blatt n von N folgt der Anlegereihenfolge, kein Umordnen.
- **Werkzeugfeld:** durch den neuen Knopf breiter; bei 800 × 600 rollt es wie schon vorher.
- **Normwerte unbestätigt:** Raster, Schriftfeldmaße, Projektionssymbol und Schriftreihe (2,5 / 3,5 / 5 nach ISO 3098; die Spec nennt 2,2 und 3,3) sind Entwurfswerte; die Normtexte zu kaufen ist Sache des Eigners.
- **Zurückgestellt aus der Prüfung:** Zeilenhöhen des Schriftfelds weichen von der Spec-Notiz „7 bis 16 mm“ ab (F6 gilt); das Spec-Wort „Randteilung am Rahmen innen“ ist mehrdeutig (die Teilstriche gehen vom Rahmen nach außen); die Meldung von `set_project_info` trägt das Präfix „Projektangaben:“.

### Handliste für den Nutzer (Prüfung von Hand, Stand 2026-10-17)
Proben liegen in `D:\Testumgebung\zeichnung-proben\blatt` (Dateinamen `<Größe>_<M1|M3>_<kurz|lang>_<Rahmen|ohneRahmen>`).
1. **Papiergrößen ansehen:** Neue Zeichnung in A4 hoch, A4 quer, A3 hoch und quer, A2, A1, A0 anlegen, jeweils bei 800 × 600 und im großen Fenster, Hell und Dunkel: Das Papier bleibt weiß, Rahmen, Raster, Schriftfeld, Tabelle und Hinweisfeld sind ganz zu sehen, nichts überlappt, nichts ragt über das Papier. A4 quer und hoch wirkt wie ein verkleinertes A3: Rahmen und Schriftfeld sind nicht größer als die Zeichnung; A0 wie ein vergrößertes A3.
2. **Fenster „Blattangaben“:** über den Knopf (Ende der ersten Zeile) und über das Menü öffnen; Felder ausfüllen und „Übernehmen“; ein Feld mit „Zurücksetzen“ zurückstellen; mit „Abbrechen“ und mit dem Kreuz schließen (nichts darf sich ändern); in einem alten, schreibgeschützten Stand ist es gesperrt.
3. **Änderungen:** 23 Änderungen anlegen (Buchstaben A bis Z ohne I, O, Q); bei der 24. kommt die Meldung „höchstens 23 Änderungen“; auf dem Blatt stehen die drei neuesten, die neueste unten (im Fenster der Hinweis „Auf dem Blatt stehen die letzten 3 Änderungen.“); Ansichten, die vor den Änderungen angelegt wurden, überdecken die volle Tabelle nicht; die Warnung bei zu langem Text erscheint unter der Zeile.
4. **Hinweise ändern:** Zeilen bearbeiten, hinzufügen (bis 6), löschen; „Eigene Hinweise für dieses Blatt“ und „Hinweise zurücksetzen“ ausprobieren; im Querformat steht das Feld neben dem Schriftfeld, im Hochformat darüber (auf jeder Größe).
5. **Neue Ansicht:** eine Ansicht hinzufügen; sie landet im freien Bereich und nicht im Schriftfeld. Grundansicht, Seitenansicht, Draufsicht und Isometrie anlegen (Erst- und Drittwinkel): die Isometrie liegt im leeren Viertel neben dem Block und überdeckt keine Ansicht (wenn nötig in kleinerem Maßstab).
6. **Blatt n/N:** drei Zeichnungen anlegen; die Blattfelder zeigen „1 von 3“ bis „3 von 3“, die Zeichnungsnummern zählen mit; Speichern unter anderem Namen ändert die automatische Nummer, eigene Texte bleiben.
7. **Alte Datei:** eine Datei aus der Zeit vor dem Schriftfeld öffnen, ansehen, das Fenster öffnen und abbrechen, speichern: Format und Inhalt bleiben alt (Format 4 oder 5). Erst „Übernehmen“ macht die Datei zu Format 6.
8. **SVG:** eine Probe in Inkscape und im Browser öffnen und auf A3 und A4 drucken: der Rahmen liegt ganz auf dem Blatt, nichts überlappt, Umlaute, Ø, ° und ± stimmen; „lang“-Proben: kein Text ragt über seine Zelle; „ohneRahmen“: nur Ansichten, kein Rahmen.
9. **DXF:** in Fusion 360 einfügen (Linien und Rahmen; Texte erscheinen dort bekanntlich nicht) und, falls vorhanden, in LibreCAD oder QCAD (Texte, Ebenen `BLATTRAHMEN`, `BLATT_MITTE`, `BLATT`). Die „lang“-Proben in LibreCAD/QCAD: kein Text ragt über seine Zelle.
10. **Projektionssymbol:** Methode 1 (Kegelstumpf links, Kreise rechts) und Methode 3 (Kreise links, Kegelstumpf rechts) im Schriftfeld ansehen und mit der Norm vergleichen.
11. **Zentriermarken und Raster:** Marken (auf A3 0,5 mm, 5 mm hinter dem Rahmen, auf anderen Formaten mal Faktor) gegen die Norm ansehen; Raster auf jedem Format 8 × 6 Felder (hoch 6 × 8).
12. **Werkzeugfeld:** bei 1100 × 700 kein Rollbalken, bei 800 × 600 rollt es.

## Teilprojekt 3c-2 – Stückliste und Positionsnummern

### Stand und Geliefertes
Stand: 2026-10-18 (Zweig `feature/stueckliste`, Basis `master` @ `84148e4`). Aus dem Zeichnungsblatt ist ein Blatt mit Stückliste und Positionsnummern geworden. Bedienung und Grenzen für Anwender: README, Abschnitt „Stückliste und Positionsnummern“. Festlegungen F1 bis F6 des Entwurfs: Spec §9, Umsetzung: Spec §10 und Plan („Abweichungen bei der Umsetzung“).

- `planora-drawing`: `parts.rs` (Körperkennung `BodyKey`, `RowOverride`, `PartsRow`, Zellen, Grenzen, Auflösung der Texte, Seitenrechnung), Tabellengeometrie in `sheet_frame.rs` (`PartsSpace`, `sheet_layout_with`, `parts_rows_per_page`), Tabellenmarkierungen in `sheet_marks.rs` (`parts_table_marks`, `parts_cell_is_cut`), `balloon.rs` (Nummernkreise als Markierungen der Art `Position`), `DrawingKind::{Views, PartsList}`, `Balloon`, SVG-Gruppe `„{Ansicht} Positionen“`, DXF-Ebenen `POSITIONEN` und `POSITIONEN_LINIE`.
- `planora-feature`: Körperkennung und Stücklistenableitung im Dokument (`body_key`, `row_of`, `parts_rows`, `parts_cells`, `set_parts_override`, `body_display_name`), Stücklistenblätter als echte Zeichnungen mit Abgleich (`add_parts_sheets`, `sync_parts_sheets`), Nummernkreise (`add_balloon`, `balloon_numbers`), Prüfungen beim Ändern und Laden.
- `planora-project`: Dateiformat 7 (nur mit Stücklistendaten oder Nummernkreisen; Format 1 bis 6 laden und speichern Byte für Byte wie bisher; eingefrorene Prüfdateien `format6_sheet_info.json` und `format7_parts.json`).
- `planora-app`: Tabelle auf dem Blatt mit Platzreserve, Abschnitt „Stückliste“ im Fenster „Blattangaben“ (neuer Fußbereich `Dialog::show_with_footer`), Stücklistenblätter im Browser mit ihren Regeln, Werkzeug „Position“ und Browser-Abschnitt „Positionen“, Ausgabe in SVG und DXF aus dem Export-Fenster mit Sperre für verlorene Nummern.
- Proben: `cargo test --release -p planora-drawing --test hand_samples_parts -- --ignored` schreibt 40 Dateien nach `D:\Testumgebung\zeichnung-proben\stueckliste` (abweichend über `PLANORA_PARTS_SAMPLES_DIR`): A4 hoch, A4 quer, A3 quer, A0 quer; Tabelle „kurz“ (3 Zeilen) und „lang“ (12 Zeilen, davon 10 auf dem Blatt, Längstexte, Sonderzeichen, 6 Hinweiszeilen); Kreise mit den Nummern 1, 12 und 200; je SVG und DXF mit und ohne Rahmen; dazu ein Stücklistenblatt mit zwei Seiten (A4 hoch, 45 Zeilen: 44 und 1), je SVG und DXF mit und ohne Rahmen. Dateinamen `<Größe>_<kurz|lang>_<Rahmen|ohneRahmen>` und `Stuecklistenblatt_Seite<n>_<Rahmen|ohneRahmen>`.
- Prüfstand: 2026-10-18, Stand der Aufgabe 9: `cargo test --workspace -j 2 --no-fail-fast`: 2538 Tests grün, 37 ausgeschaltet, 0 Fehler, 273 Testläufe (3c-1 endete bei 2387 grünen und 34 ausgeschalteten); `cargo fmt --all --check` und `cargo clippy --workspace --all-targets -- -D warnings` sauber; `cargo deny check`: advisories, bans, licenses und sources ok (mit den vermerkten Ausnahmen zu den drei zbus-Hinweisen); Kern, Netz-Quelltext, STEP und Austausch gegenüber `945fa67` und `84148e4` unverändert; `Cargo.toml`, `Cargo.lock` und `deny.toml` gegenüber `84148e4` unverändert, keine neuen Pakete.

### Festlegungen G1 bis G15 und F1 bis F6 mit Abweichungen
Die Festlegungen stehen im Plan (G1 bis G15) und in der Spec §9 (F1 bis F6); der Nutzer hat sie bestätigt (Ledger: Boolesche Benennung bleibt wie heute, G2). Hier steht, wo die Umsetzung etwas genauer festlegt oder abweicht.

- **G1/F1 Körperkennung:** `BodyKey { feature, index }` folgt Verschieben, Verrunden und dem Ziel einer Booleschen Operation. Abweichung der Prüfung: Der Kern liefert je Booleschen Operation genau einen Körper, ein echter Split ist nicht erzeugbar; der Fall „Ergebnis-Index 1 und höher“ ist deshalb nur rein strukturell getestet (`boolean_split_has_own_key`), nicht an einem echten Körper.
- **G2 Benennung:** Das Ergebnis einer Booleschen Operation behält den Namen der Operation („Vereinigung 1“). Folge: Ein eigener Text, der gesetzt wurde, als der Körper noch „Platte“ hieß, bleibt an der Zeile, die jetzt „Vereinigung 1“ heißt (die Kennung hat sich nicht geändert). Die Anzeigenamen-Regel zog dazu unverändert aus `state.rs` nach `Document::body_display_name`.
- **G3 Zeilen:** Nur sichtbare, nicht unterdrückte exakte Körper; Netz-Körper stehen nicht in der Stückliste (F6) und bekommen keinen Kreis. Überschreibungen für unsichtbare Körper bleiben im Dokument, bis sie zurückkommen; höchstens 1000. Eine Überschreibung für einen Körper, den es gar nicht gibt, wird in einer fremden Datei toleriert (bleibt als verlorener Eintrag), wie über die Dokument-Schnittstelle.
- **G4 Spalten, G5 Lage:** wie im Plan. Anordnung und Maßstabsvorschlag rechnen immer mit 10 Zeilen, gezeichnet wird mit der tatsächlichen Zahl (höchstens 10).
- **G6 Stücklistenblatt:** Der freie Bereich (`SheetLayout.free`) ist dort die Tabellenfläche (`free == parts`). Er darf nicht für Anordnung oder Maßstabsvorschlag benutzt werden (die 3c-1-Regel F8 ergäbe dort einen leeren Bereich); auf Stücklistenblättern gibt es keine Ansichten. Zeilen je Blatt 44 (hoch) und 26 (quer) auf jedem Papier.
- **G7/F3 Seiten:** Zusätzlich zum Plan sind die Namen „Stückliste“ bis „Stückliste (8)“ reserviert (8 = Seiten bei 200 Zeilen und 26 je Blatt): Solange es Stücklistenblätter gibt, nehmen andere Zeichnungen diese Namen nicht an; `add_parts_sheets` lehnt ab, wenn einer der Folgenamen schon vergeben ist (nichts wird geändert); eine Zeichnung mit so einem Namen aus einer alten Datei bleibt änderbar, ein Folgeblatt mit belegtem Namen wird nicht angelegt (kein Absturz, nichts überschrieben). Der Abgleich ändert nur den Zähler der Zeichnungsänderungen (`drawing_edits`), nie den der Modelländerungen; er zählt nur bei tatsächlicher Wirkung (+1 je Abgleich). `add_parts_sheets`, `delete_drawing` und `update_drawing` zählen je genau +1. `update_drawing` ändert nie die Art; der Name eines Stücklistenblatts ist nie änderbar; eine Änderung an einem Folgeblatt, die der Abgleich zurücknimmt, zählt 0. Das Laden prüft (Seiten genau 1 bis k ohne Lücke, Namen = `parts_sheet_name(Seite)`, keine Ansicht, kein Maß, kein Kreis auf einem Stücklistenblatt), gleicht aber nicht ab. Bei `MAX_DRAWINGS` bleibt die Datei ladbar; der Abgleich legt dann nichts mehr an.
- **G8/F4 Schalter:** `SheetInfo.parts_list`, einmal beim Anlegen auf „mehr als ein sichtbarer Körper“ gesetzt; alte Zeichnungen (Format 6 und älter) aus. Auf einem Stücklistenblatt wird der Schalter ignoriert (auch wenn eine Datei ihn auf an hat; das Fenster zeigt ihn dort nicht). Kein bestehender App-Test musste seine Erwartung ändern: `sheet_view`, `drawing_ui`, `sheet_dialog` und `wide_dialogs` legen Zeichnungen nur in Dokumenten mit einem Körper an.
- **G9/F2 Kreisdaten:** Anker relativ zur Hüllbox der Ansicht, beim Setzen auf [0, 1] begrenzt; Versatz in Blatt-mm vom Anker, Standard `(10, 10) · f`, beim Setzen so begrenzt, dass die Kreismitte auf dem Blatt bleibt; der Betrag des Versatzes ist durch die Positionsgrenze der Zeichnung beschränkt (geprüft als Betrag, nicht je Richtung). Ansicht löschen löscht ihre Kreise. Die Kennung der Kreise zählt weiter, auch nach dem Löschen (in Datei und Dokument), damit eine gelöschte Kennung nicht neu vergeben wird.
- **G10 Zeichnung:** Kreis und Punkt sind volle Kreisbögen (`Mark::Arc` 0 bis 2π, in DXF `CIRCLE`); die Hinweislinie entfällt, wenn der Versatz höchstens der Kreisradius ist (liegt der Anker im Kreis); die Zahl sitzt mittig und wird in 0,8 · Ø eingepasst (kleiner bis ohne Untergrenze, nie gekürzt; „200“ braucht Höhe 2,04 · f), die Grundlinie bleibt `Mitte − Höhe/2` mit der eingepassten Höhe. Verlorene Kreise: dieselbe Geometrie als `MarkKind::Lost` mit „?“ (SVG: in der Gruppe `„{Ansicht} Maße“`, DXF: Ebene `MASSE_FEHLT`; die Exportsperre verhindert die Ausgabe ohnehin).
- **G11 Werkzeug:** „Position“ steht beschriftet in Zeile 2 des Werkzeugfelds hinter dem Maß-Auswahlfeld. Nach der Messung (1100 × 700) ging das nur, indem „Blatt einpassen“ ein reines Symbol neben den Zoom-Knöpfen wurde (Entscheid des Controllers nach dem Review; Name und Tooltip „Blatt einpassen“) und das Maß-Auswahlfeld eine feste Breite (die des längsten Eintrags) bekam. Bei 800 × 600 rollt das Werkzeugfeld weiter wie vorher. Entf löscht den gewählten Kreis nur, wenn die Zeichnung bearbeitbar ist, niemand tippt (`viewport::typing`), kein Fenster oder Menü offen ist und der letzte Mausdruck auf dem Blatt war (nicht auf einer Browserzeile). „Maß“ und „Position“ schließen sich aus. Kreise werden vor Maßen und Ansichten getroffen (auch ohne Werkzeug), außer solange „Maß“ an ist; der Treffer reicht 6 Bildpunkte (`PICK_PX`) über den Kreisrand, bei mehreren der nächste Rand. Das Setzen verlangt eine Zeile in der Stückliste (`row_of`) und lehnt sonst mit „Bitte eine Linie eines Teils anklicken.“ ab; Isometrie ist erlaubt. Das Ziehen ändert das Dokument einmal beim Loslassen (+1), nicht je Bild; ein Zurückziehen an den Start ändert nichts.
- **G12 Ausgabe:** Kreise immer (auch ohne Rahmen). DXF-Ebenen `POSITIONEN` und `POSITIONEN_LINIE` (Farbe 7, nur bei Benutzung angelegt); die Strichstärken sind, anders als die A3-Zahlen 35 und 25 des Plans, mit dem Blattfaktor skaliert wie die Blattebenen (A3 35/25, A4 25/18; Entscheid: angenommen, die Vorgabe „Linien mal f“ gilt). Dazu legt der DXF-Schreiber die Blattebenen jetzt je Strichstärke an (`sheet_heavy/medium/thin`) statt alle drei, sobald es einen Rahmen gibt: ein ganzer Rahmen nutzt alle drei (Ausgabe unverändert, Goldene Dateien gleich), ein Stücklistenblatt ohne Rahmen nur `BLATT`. Das war nötig, damit ohne Rahmen keine leere Ebene `BLATTRAHMEN` entsteht. Die SVG-Gruppe `Blattrahmen` bleibt auch beim Stücklistenblatt ohne Rahmen bestehen und enthält dann nur die Tabelle.
- **DXF-Breitenzuschlag:** Die Zellen der Tabelle und die Zahlen der Kreise werden im DXF gegen die 1,45-fache Schätzung eingepasst (`DXF_TEXT_WIDTH_FACTOR`, wie die Rahmentexte; die Zahl nachträglich über `balloon_text_dxf`, SVG unverändert). Die DXF-Prüfung der Zeichen und der Länge (255) deckt auch die Zellen ab; die Meldung nennt Zeile und Spalte, „Stückliste, Pos. {n}, {Spalte}“, wobei n der Pos.-Text der Zeile ist (auf Blatt 2 also etwa 45); Zeichen werden vor der Länge geprüft.
- **G13 Exportsperre:** „Die Positionsnummer in der Ansicht „{Ansicht}“ gehört zu einem Teil, das es nicht mehr gibt. Bitte die Positionsnummer löschen und neu setzen.“; geprüft ganz zu Beginn, vor Ansichten und Blattbau, der erste verlorene Kreis in der Reihenfolge der Zeichnung; nichts wird geschrieben. Die ältere Schnittstelle `prepare_drawing`/`_framed` ohne Nummern behandelt jeden Kreis als verloren (in Debug-Läufen zusätzlich `debug_assert!`); der Weg mit Nummern ist `prepare_drawing_parts`.
- **G14 Browser:** wie im Plan; zusätzlich fällt ein offenes oder gewähltes Folgeblatt beim Löschen der Seite 1 sofort aufs Modell zurück (`forget` für jede verschwundene Kennung). „Blatt n von N“ verschiebt sich, wenn ein Folgeblatt entsteht oder verschwindet.
- **G15 Fenster:** Der Abschnitt „Stückliste“ ist kompakt: Schalter, Papierwahl (nur solange es kein Stücklistenblatt gibt) und Knopf oben, darunter eine Tabelle mit einer Zeile je Teil (Pos. 40 px, Menge 64 px, Rest im Verhältnis 78:45:30); „Zurücksetzen“ ist ein Kreuz-Symbol in der Zeile (nur bei eigenem Text); „n/Höchstzahl“ steht als Hover-Text. „Übernehmen“, „Abbrechen“ und die Meldung stehen in einem Fußbereich unter dem rollenden Inhalt (`Dialog::show_with_footer`, neu in 3c-2; das Fenster „Blattangaben“ benutzt ihn jetzt insgesamt, `Dialog::show` ist unverändert). Folge für einen bestehenden Test: `wide_dialogs::sheet_dialog_stays_inside_and_scrolls_at_800x600` erwartet „Übernehmen“ nun immer sichtbar im Fußbereich (vorher erst nach dem Rollen). „Übernehmen“ prüft alles vorab (Projekt, Blatt, jede Überschreibung, Zahl ≤ 1000) und schreibt erst dann; ein „Übernehmen“, das Blattangaben und mehrere Überschreibungen ändert, zählt mehrere Änderungen (eine für die Blattangaben, eine je Überschreibung). Im schreibgeschützten Stand (alte Version) ist das Fenster gesperrt, im Skizzenmodus der Knopf. Bei mehr als 200 Zeilen sind nur die ersten 200 bearbeitbar.
- **F5 Umbenennen:** Gruppiert wird nach dem Körpernamen, nicht nach einer überschriebenen Benennung; die Nummer richtet sich nach der Zeile, Kreise am mittleren Körper einer Dreierzeile haben die Zeilennummer.
- **Dateiformat 7:** Format 7 steht nur bei Überschreibungen, Stücklistenblatt, Schalter an oder Nummernkreisen; sonst bleibt jede Datei Byte für Byte wie bisher (Test mit eingefrorenen Dateien Format 6 und 7). Gespeichert wird der Schalter nur, wenn er an ist; der Kennungszähler der Kreise, wenn das Format 7 ist und es Kreise gibt oder der Zähler über 1 steht (nie in Dateien, die nicht Format 7 sind). Daten von Format 7 in einer Datei mit niedrigerer Nummer sind eine benannte Beschädigung („Stücklisten und Positionsnummern gibt es erst ab Dateiformat 7“). Grenzen beim Lesen: höchstens 1000 Überschreibungen (die Lesegrenze liegt beim Streamen), höchstens 200 Kreise je Zeichnung (gelesen werden bis 201, dann Fehler); Körperindex `u32`; eine Überschreibung ganz aus „automatisch“ ist eine benannte Beschädigung; doppelte Schlüssel ebenso. Format 8 ist „zu neu“ (vorher Format 7). Das Laden verändert nichts (Änderungszähler 0, Blätter nicht abgeglichen).
- **Fortschritt im Export:** Ein Stücklistenblatt braucht keine Ansicht; der Fortschrittsbalken hat mindestens den Gesamtwert 1.
- **Kreise beim Neuberechnen:** Beim Neuberechnen einer Ansicht liegen für einen Moment keine Ergebnisse vor, dann fehlt die Hüllbox und die Kreise verschwinden kurz (wie die Linien); danach erscheinen sie wieder an derselben Stelle.

### Grenzen
- **Maße nicht aus der Norm belegt (Spec §8):** Kreis Ø 8 mm, Zahl 3,5 mm, Kreislinie 0,35, Hinweislinie 0,25, Punkt r 0,5; Tabelle: Zeilenhöhe 7, Schrift 3,5, Spalten 12 / 78 / 15 / 45 / 30 (Summe 180, Schriftfeldbreite); Zeilenzahl 10 auf dem Blatt. ISO 7573 und die Zeichnungsnormen waren nicht verfügbar. Die Zahlen folgen der Praxis; wer die Norm prüft, trägt Abweichungen hier nach.
- **Gleichnamige Körper mit verschiedener Geometrie bleiben eine Zeile** (gleich heißt gleicher Name); der Nutzer benennt sie anders.
- **Benennung einer Booleschen Operation (Q1/G2):** Das Ergebnis heißt wie die Operation; ein eigener Text, der beim Zielkörper stand, bleibt an der Zeile, die jetzt „Vereinigung n“ heißt. Soll die Benennung dem Ziel folgen, betrifft das nur `Document::body_display_name` und die Namen im Browser.
- **Laden gleicht die Stücklistenblätter nicht ab:** Eine Datei, deren Körperzahl nicht zu ihren Stücklistenblättern passt, wird unverändert geladen; erst die nächste Änderung am Modell gleicht ab.
- **Tabelle des Stücklistenblatts ohne Rahmen:** liegt in der SVG-Gruppe `Blattrahmen` und in DXF auf `BLATT` (kein eigener Gruppenname). Ein Programm, das alles in `Blattrahmen` ausblendet, blendet auch diese Tabelle aus.
- **Freier Bereich auf Stücklistenblättern:** `SheetLayout.free` ist dort die Tabellenfläche (`free == parts`) und darf nicht für die Anordnung oder den Maßstabsvorschlag benutzt werden.
- **Netz-Körper (F6):** stehen nicht in der Stückliste und bekommen keine Kreise.
- **Kreise folgen nicht den Kanten (F2):** Der Anker ist ein Punkt relativ zur Hüllbox der Ansicht; ändert sich die Form stark, kann der Punkt neben der angeklickten Linie liegen. Nur das Verschwinden des Körpers macht den Kreis verloren.
- **Werkzeug-Körper einer Booleschen Operation:** seine Kreise sind danach verloren (F1); das lässt sich nicht automatisch auf das Ziel umhängen („Neu zuweisen“ = löschen und neu setzen).
- **Stückliste je Dokument, nicht je Blatt:** Zeilentexte gelten für alle Blätter; kein Umordnen von Hand, kein automatisches Setzen aller Kreise (Nicht-Ziele der Spec).
- **Folgeblätter erben Papier und Ausrichtung** von Seite 1; Seite 1 löschen entfernt alle.
- **Eigene Blattangaben eines Folgeblatts gehen verloren,** wenn es verschwindet (weniger Zeilen, etwa ein unterdrückter Körper oder verschmolzene Zeilen); es kommt mit neuer Kennung und automatischen Texten wieder.
- **DXF und gekürzte Tabellentexte:** Der Breitenzuschlag kann einen Tabellentext im DXF mit „…“ kürzen, ohne dass das Fenster warnt (die Warnung folgt der SVG-Regel). Eine sehr lange Benennung aus Umlauten (etwa 30 und mehr Zeichen) sperrt den DXF-Export mit der benannten Meldung.
- **Kein Umordnen der Körper in der App:** Es gibt keinen Weg, Körper umzuordnen; die Reihenfolge der Zeilen ändert sich nur durch Löschen, Umbenennen oder Neuanlegen von Körpern.
- **Randfall Export:** Ein Stücklistenblatt lässt sich in einem Dokument ohne feste Körper nicht ausgeben.
- **Aufwand je Bild:** `parts_rows` ist quadratisch in der Zeilenzahl (bei 200 Zeilen noch unauffällig, siehe Zahlen); Kreise und Nummern sind je (Instanz, Änderungsstand) zwischengespeichert, der Abgleich der Zellen für das Blatt ebenso.
- **1 Bildpunkt und Isometrie (aus 3c-1, unverändert):** Tabellentexte sind Blattmarkierungen und werden unter einem Bildpunkt auf einen Bildpunkt angehoben; die Zahlen in den Kreisen (Art `Position`) gehören nicht dazu und verschwinden, wenn sie kleiner als ein Bildpunkt würden. Die isometrische Ansicht ist bei fast blattfüllenden Teilen bestmöglich (kann den freien Bereich überragen); dasselbe gilt für „Rechts“ im Erstwinkel, das links vom Block steht und den freien Bereich verlassen kann.
- **Normwerte der DXF-Strichstärken:** Die Stärken der Ebenen `POSITIONEN` und `POSITIONEN_LINIE` skalieren mit dem Blattfaktor (siehe G12); auf A4 sind es 25 und 18.
- **Fusion 360 zeigt DXF-Texte nicht** (bekannt aus Teilprojekt 12): Tabelle und Zahlen sind dort nur Linien; LibreCAD oder QCAD zeigen die Texte.
- **Überlebende Mutationen (alle gleichwertig, im Bericht der jeweiligen Aufgabe begründet):** SheetCache ohne Zellvergleich und ohne Kreisliste (jede Änderung erzeugt ohnehin ein neues Ergebnis); zweite Sperre im Browser-Menü; Körperindex über alle statt nur exakte Körper (Netz-Körper stehen immer hinter den exakten); Vorprüfung der Kreisgrenze in `place` und `add_balloon` (`validate_drawing` meldet dasselbe); „Ziehen schreibt auch ohne Änderung“ (`update_drawing` zählt eine unveränderte Zeichnung nicht); Prüfung „Körper hat eine Zeile“ beim Setzen (jeder exakte sichtbare Körper hat eine).
- **Abweichung im Vorgehen:** In den Aufgaben 5, 6 und 8 wurde die Umsetzung vor dem ersten Testlauf geschrieben (kein rotes Vorab-Lauf); die Tests wurden stattdessen durch Mutationen auf Wirksamkeit geprüft (alle gleichwertigen Überlebenden stehen oben). Ein Test (`closing_a_tab_while_a_drawing_is_written_leaves_no_helper_file`) war einmal rot und ließ sich nicht wieder erzeugen; vermutete Ursache ist das Lesen der Zieldatei, während der Schreib-Thread sie ersetzt (Windows). Das Lesen wird jetzt bis zu 5 Sekunden wiederholt.
- **Dateileser, Randfälle:** Ein `null` als Anker in einer fremden Datei ergibt eine Meldung des Dateilesers ohne Zeichnungsnamen; ein Anker außerhalb [0, 1] bekommt die benannte Meldung der Prüfung. Beim Speichern wird ein nicht endlicher Anker verweigert.
- **Hand-Test im echten Fenster** (`tools/gui-drive.ps1`) durch den Controller steht noch aus und braucht die Freigabe des Nutzers.

### Zahlen
- Zeilen je Stücklistenblatt: 44 (hoch) und 26 (quer) auf jedem Papier; 200 Zeilen insgesamt ergeben höchstens 8 Blätter (bei 26 je Blatt); 45 Zeilen im Hochformat ergeben 2 Blätter (44 und 1); im Test `many_parts_fill_follow_pages_and_page_numbers_move` (45 Körper, A4 hoch) lesen die Blätter „1 von 4“ bis „4 von 4“, und beim Löschen eines Körpers wird aus der Zählung „… von 3“.
- Zeit je Bild (Debug-Build, nicht Release, 1400 × 900, gemessen mit den ausgeschalteten Tests `frame_cost`): Blatt mit Tabelle bei 45 Körpern 2,3 ms, bei 200 Körpern 6,4 ms; Fenster „Blattangaben“ bei 45 Körpern 3,8 ms (oben) und 4,4 ms (bei den Zeilen), bei 200 Körpern 8,8 und 10,4 ms (vor der Zwischenspeicherung etwa 45 ms bei 200 Körpern). Zeichnung ohne Kreise 2,5 ms; mit 200 Kreisen in Ruhe 5,6 ms, beim Ziehen 5,8 ms (vorher 6,2 und 7,1 ms). Der Rest ist das Zeichnen der 200 Kreise und Browserzeilen. Release wurde nicht gemessen (der Bau dauert auf diesem Rechner zu lange).
- Geprüfte Grenzen: 26/27, 52/53, 44/45 und 200/201 Zeilen; 1000/1001 Überschreibungen; 200/201 Kreise; Zeilen- und Kreisgrenzen im Dateileser (200 000 Überschreibungen und 300 000 Kreise werden abgeschnitten, nicht gespeichert).
- Mutationsproben: über 250 einzelne Mutationen über die neun Aufgaben (Berichte unter `.superpowers/sdd/2026-10-18-stueckliste/`), alle bis auf die oben genannten gleichwertigen Überlebenden rot.

### Handliste für den Nutzer (Prüfung von Hand, Stand 2026-10-18)
Proben liegen in `D:\Testumgebung\zeichnung-proben\stueckliste` (Namen siehe oben).

**A. Tabelle auf dem Blatt**
1. Projekt mit drei Teilen anlegen, zwei davon gleich benannt (zum Beispiel zweimal „Bolzen“, einmal „Platte“). Menü „Zeichnung“ → „Neue Zeichnung …“: Auf dem Blatt steht über dem Schriftfeld die Tabelle mit Kopfzeile unten, Pos. 1 „Platte“ oder „Bolzen“ je nach Reihenfolge, der Bolzen mit Menge 2 in einer Zeile. Die Ansichten liegen nicht unter der Tabelle, auch nicht, wenn man weitere Ansichten hinzufügt.
2. Ein Teil umbenennen: Die Tabelle folgt (und die Kreise bleiben am Teil). ein früheres Teil löschen, Teil verschieben, verrunden: Tabelle und Nummern stimmen. Zwei Teile vereinigen: Die Zeile heißt jetzt wie die Operation („Vereinigung 1“), der Werkzeug-Körper ist aus der Liste verschwunden und seine Kreise sind rot.
3. Eine Zeichnung in einem Projekt mit nur einem Teil: keine Tabelle (Schalter aus). In einer alten Zeichnung (Datei vor diesem Stand) mit mehreren Teilen: keine Tabelle, nichts verändert.

**B. Fenster „Blattangaben“, Abschnitt „Stückliste“**
4. Fenster öffnen (Knopf „Blattangaben“ oder Menü): Der Abschnitt „Stückliste“ zeigt den Schalter, eine Zeile je Teil mit grauen Vorgabetexten. Benennung, Menge, Werkstoff und Bemerkung ändern, „Übernehmen“: Die Tabelle zeigt die Texte. Das Kreuz in der Zeile stellt auf automatisch zurück (nur sichtbar, wenn die Zeile eigenen Text hat). „Abbrechen“ und das Kreuz oben verwerfen alles. Zu lange Texte: Warnung unter der Zeile, auf dem Blatt kleiner und mit „…“.
5. In einem Feld Entf, Esc, „d“ und „+“ tippen: Es landet im Feld; nichts anderes passiert (kein Kreis wird gelöscht, kein Werkzeug beendet).
6. Schalter „Stückliste auf diesem Blatt“ ausschalten, übernehmen: Die Tabelle verschwindet, der Platz wird frei. Wieder einschalten: Sie ist zurück, ohne dass sich Ansichten verschoben haben.
7. Viele Teile (45 Körper, zum Beispiel ein Muster): Der Abschnitt bleibt bedienbar (Fußbereich mit „Übernehmen“ und „Abbrechen“ immer sichtbar), bei mehr als 10 Teilen steht die Warnung, dass die übrigen auf das Stücklistenblatt gehören; auf dem Blatt stehen die ersten 10.
8. Fenstergröße 800 × 600 und ein großes Fenster, jeweils Hell und Dunkel: nichts ragt aus dem Fenster, der Fußbereich bleibt sichtbar, die Zeilen sind einzeilig und lesbar.

**C. Stücklistenblatt**
9. Im Abschnitt Papier wählen (A4 hoch, A3 quer, …) und „Stücklistenblatt erzeugen“: Ein neues Blatt „Stückliste“ erscheint mit Rahmen, Schriftfeld (Dokumentart „Stückliste“) und großer Tabelle, Kopfzeile oben. Der Knopf heißt jetzt „Stücklistenblatt öffnen“.
10. Mit 45 Teilen auf A4 hoch: Blatt „Stückliste“ hat 44 Zeilen, es gibt ein Blatt „Stückliste (2)“ mit einer Zeile; „Blatt n von N“ zählt alle Blätter. Auf A3 quer sind es 26 Zeilen je Blatt (bei mehr Teilen entsprechend mehr Blätter).
11. Teile löschen, bis es nur noch 44 sind: Das Folgeblatt verschwindet von selbst (offen gewesen: Die Ansicht fällt aufs Modell zurück, kein Absturz). Wieder ein Teil hinzufügen: Es entsteht wieder.
12. Im Browser: „Umbenennen“ ist bei allen Stücklistenblättern gesperrt, „Löschen“ beim Folgeblatt gesperrt (mit Hinweis). Das erste Blatt löschen: Rückfrage „Stückliste mit 2 Blättern löschen?“, danach sind alle weg.
13. Auf dem Stücklistenblatt: „Ansicht hinzufügen“ ist gesperrt (mit Hinweis), das Werkzeug „Position“ ebenso.

**D. Positionsnummern**
14. Werkzeug „Position“ (Zeile 2 des Werkzeugfelds, hinter dem Maß-Auswahlfeld): Auf eine sichtbare Kante, eine verdeckte Kante, einen Umriss (Zylinder) und einen Bogen klicken: jeweils ein Kreis schräg oberhalb, die Hinweislinie endet mit einem Punkt am Klickpunkt, die Nummer stimmt mit der Tabelle überein. Daneben klicken: rote Meldung „Bitte eine Linie eines Teils anklicken.“
15. Kreis ziehen: Der Kreis folgt der Maus, die Hinweislinie bleibt am Teil; nach dem Loslassen bleibt er. Auch ohne Werkzeug: Kreis anklicken (wählt) und ziehen. Entf löscht den gewählten Kreis; Esc hebt die Wahl auf, und bei eingeschaltetem Werkzeug beendet Esc das Werkzeug. Im Browser den Abschnitt „Positionen“ ansehen: Kreise je Ansicht, Klick wählt, rechte Maustaste → „Löschen“.
16. Zwei gleich benannte Teile: Kreise an beiden zeigen dieselbe Nummer. Eines umbenennen: Es bekommt eine eigene Zeile und Nummer, die Nummer des anderen bleibt.
17. Teil umbenennen, verschieben, verrunden: Der Kreis bleibt am Teil. Teil löschen (oder unterdrücken): Der Kreis wird rot mit „?“, im Hinweisfeld steht die Zahl der verlorenen Kreise, beim Export meldet das Programm „Die Positionsnummer in der Ansicht … gehört zu einem Teil, das es nicht mehr gibt …“ und schreibt nichts. Kreis löschen: Der Export geht wieder.
18. Vereinigen zweier Teile: Kreise am Ziel bleiben, Kreise am Werkzeug-Körper werden rot.
19. In der Ansicht mit dem Maß-Werkzeug „Maß“ einschalten: „Position“ geht aus (und umgekehrt).
20. Bei 1100 × 700: kein Rollbalken im Werkzeugfeld; bei 800 × 600 rollt es wie vorher.
21. Mit 200 Kreisen in einer Zeichnung: der 201. Klick meldet „Es sind höchstens 200 Positionsnummern je Zeichnung möglich.“ in Rot (die Zahl der Kreise bleibt 200). Die Zeichnung bleibt flüssig.

**E. Speichern, Öffnen, alte Dateien**
22. Mit Stückliste, Stücklistenblatt, eigenen Texten und Kreisen speichern, schließen, wieder öffnen: alles ist da, die Kreise sitzen an denselben Teilen, die Kennung weiterer Kreise zählt weiter. Als Version einer älteren Fassung zurückholen (Versionsliste): die Daten kommen mit.
23. Eine alte Datei (vor diesem Stand, Format 4 bis 6) öffnen, ansehen, das Fenster „Blattangaben“ öffnen und abbrechen, speichern: Format und Inhalt bleiben alt. Erst eigene Stücklistentexte, ein Stücklistenblatt oder Kreise machen die Datei zu Format 7. Ein älteres Programm lehnt Format 7 mit einer Meldung ab.
24. Schreibgeschützte alte Version (Versionsliste): Fenster und Werkzeuge sind gesperrt, Kreise lassen sich wählen, nicht ändern.

**F. Ausgabe**
25. „Datei → Exportieren …“, Zeichnung als SVG, mit und ohne „Blattrahmen mit ausgeben“: Mit Rahmen steht die Tabelle im Blatt, ohne Rahmen fehlt sie; die Kreise sind in beiden Fällen da. Das Stücklistenblatt: mit Rahmen die ganze Seite, ohne Rahmen nur die Tabelle. Jede Seite des Stücklistenblatts wird einzeln gewählt.
26. SVG im Browser und in Inkscape öffnen; auf A4 und A3 drucken: nichts überlappt, nichts ragt aus Zelle oder Kreis, Umlaute, Ø, °, ± und „&“ stimmen. Proben „lang“: auch die Längstexte bleiben in ihren Zellen; die Nummer „200“ passt in den Kreis (kleiner als „7“).
27. DXF in LibreCAD oder QCAD öffnen: Tabelle und Nummern in den Zellen und Kreisen, Ebenen `POSITIONEN`, `POSITIONEN_LINIE` und die Blattebenen; kein Text ragt über seine Zelle (Zuschlag für CAD-Schriften). In Fusion 360: Linien und Kreise, Texte erscheinen dort bekanntlich nicht.
28. Eine Zelle mit einem Zeichen außerhalb des DXF-Zeichensatzes (zum Beispiel ein Gesicht-Zeichen): Der DXF-Export meldet Zeile und Spalte („Stückliste, Pos. 3, Benennung“) und das Zeichen mit seiner Nummer; SVG geht.
29. Druck der Proben auf A4 und A3: die Tabelle liegt über dem Schriftfeld und überlappt kein Hinweisfeld, keine Änderungstabelle und keine Ansicht; auf A0 skaliert alles gleich.

## Teilprojekt 3c-3 – PDF-Ausgabe

### Stand und Geliefertes
Stand: 2026-10-19 (Zweig `feature/pdf-ausgabe`, Basis `master` @ `580af2e`). Zeichnungen lassen sich als PDF ausgeben, das gewählte Blatt oder alle Blätter des Dokuments in einer Datei, mit oder ohne Blattrahmen. Bedienung und Grenzen für Anwender: README, Abschnitt „Zeichnung als PDF“. Entwurf: `docs/superpowers/specs/2026-10-18-pdf-ausgabe-design.md` (Festlegungen der Umsetzung in §9), Plan: `docs/superpowers/plans/2026-10-18-pdf-ausgabe.md` (P1 bis P14, „Abweichungen bei der Umsetzung“).

- `planora-drawing`: `pdf_font.rs` (WinAnsi-Zeichensatz mit 219 zulässigen Zeichen, 224 Helvetica-Breiten aus der Adobe-Datei `Helvetica.afm`), `pdf.rs` (Schreiber von Hand, PDF 1.4, unkomprimiert, ohne neue Bibliothek: `pdf_bytes`, `write_pdf`, `PdfMeta`, `pdf_char_message`), `DrawingFormat::Pdf` und `export_pdf_file` in `file.rs` mit der gemeinsamen Hilfsdatei-Logik `write_via_temp` (auch für SVG und DXF, deren Bytes und Ablauf unverändert sind), `Mark::Text::fit_width` (Breite der Zelle, in die ein Text eingepasst wurde; SVG und DXF ignorieren sie).
- `planora-app`: Format „Zeichnung als PDF“ im Export-Fenster (achtes Format), Wahl „Gewähltes Blatt“ / „Alle Blätter“, Zeichenprüfung mit Feldnamen, Berechnung aller Blätter im Hintergrund mit Fortschritt „Blatt n von N“ und Sperre je Blatt, Dokumenteigenschaften (`pdf_meta`).
- Tests: eigener PDF-Leser (`tests/common/pdf_read.rs`, nur `std`), Tests `stage25_pdf_font`, `stage26_pdf`, `stage27_pdf_svg` (Gleichheit mit SVG, Golden-Dateien `pdf_cylinder_a4_landscape.pdf` und `pdf_box_frame_a3_portrait.pdf`, SVG-Golden-Datei `svg_splines.svg` als Schutz der gemeinsam genutzten Kurvennäherung), `stage17_file` (Datei), `tests/export_pdf.rs` in der App (40 Tests). `.gitattributes`: `*.pdf -text` für die Golden-Dateien.
- Proben: `cargo test --release -p planora-drawing --test hand_samples_pdf -- --ignored` schreibt 62 Dateien nach `D:\Testumgebung\zeichnung-proben\pdf` (abweichend über `PLANORA_PDF_SAMPLES_DIR`): 27 PDF, 34 SVG und `ERWARTUNG.txt`. Dazu gehören `Blatt_<A4hoch|A4quer|A3quer|A0quer>_<Rahmen|ohneRahmen>` (Quader mit Bohrung in Vorne, Oben, Rechts und Isometrie, Mittellinien, Maße „40“, „20 −0,1“ und „Ø 10 ± 0,1“), `Stueckliste_<Größe>_<kurz|lang>_<Rahmen|ohneRahmen>` (3 Zeilen bzw. 12 Zeilen mit Längstexten und Sonderzeichen, davon 10 auf dem Blatt; Positionsnummern 1, 12 und 200), `Stuecklistenblatt_<Rahmen|ohneRahmen>` (A4 hoch, 45 Zeilen, zwei Seiten) und `alle-blaetter.pdf` (6 Seiten: A4 hoch, A3 quer mit Stückliste, Stücklistenblatt Seite 1 und 2, A0 quer, A4 quer). Neben jeder PDF liegt die SVG (bei mehreren Seiten je Seite eine: `…_Seite<n>.svg`).
- Prüfung der Proben mit Poppler (`pdfinfo`, `pdftoppm`, `pdftotext`): alle 27 PDF öffnen ohne Meldung, Seitenzahl und Seitengröße stimmen (A4 595,28 × 841,89 pt, A3 1190,55 × 841,89 pt, A0 3370,39 × 2383,94 pt), Titel und Datum stehen in den Eigenschaften; zwei Seiten (A4 hoch ohne Stückliste, A4 quer mit 12 Zeilen und Nummernkreisen) wurden als Bild angesehen: Rahmen, Raster, Schriftfeld, Tabelle, Kreise „1“, „12“, „200“, Maße mit „Ø“ und „±“ und gedrehter Text „20 -0,1“ stimmen. Das ersetzt die Prüfung von Hand in Edge, Chrome und einem weiteren Betrachter nicht (Handliste unten).

### Festlegungen P1 bis P14 mit Abweichungen
Die Festlegungen stehen im Plan. Was beim Bauen anders wurde:
- **P6 Stauchung:** Der Plan staucht gegen die Schätzung des Blattmodells (F1/Q1). Das wurde nach der Prüfung geändert: Gestaucht wird nur gegen die Breite der Zelle, in die ein Text eingepasst wurde (`Mark::Text::fit_width`, gesetzt für Schriftfeld, Tabellen, Stückliste und Nummernkreise). Grund: Die Schätzung zählt „1“ mit 0,3·h, die Helvetica hat 0,556·h; gegen die Schätzung wären kurze Texte wie „1“ oder „1:1“ sichtbar schmal geworden. Maße, Raster und Beschriftungen werden nie gestaucht. Der Faktor `Tz` wird mit der geschriebenen (gerundeten) Schriftgröße bestimmt und auf vier Stellen abgerundet (der Plan sagt „gerundet“): sonst wäre ein Text bis etwa 1e-4 mm breiter als seine Zelle. Der Rechenweg des Blattmodells ändert sich dafür nicht; SVG- und DXF-Bytes und alle bisherigen Golden-Dateien sind unverändert.
- **P3 Ecken:** `0 j` (spitze Ecken) statt `1 j`, weil SVG ohne `stroke-linejoin` spitz zeichnet; die Enden bleiben rund (`1 J`) wie `stroke-linecap="round"`. Die Gehrungsgrenze der PDF (10) ist größer als die der SVG (4); es betrifft nur Ecken unter etwa 29° innerhalb eines Linienzugs.
- **P5 Kurven:** Eine Spanne von `Mark::Arc` über einen Vollkreis hinaus wird auf einen Vollkreis begrenzt (vier Stücke, geschlossen). Sonst wie geplant; die Gleichheit mit SVG ist über alle Bogenstücke und Splines getestet.
- **P9 und Ergebnisse je Format (Aufgabe 4):** Das Export-Fenster merkt sich ein berechnetes Ergebnis je Zeichnungsformat (SVG, DXF, PDF getrennt), nicht nur eines. Der Plan wollte „zurück auf SVG ohne Neuberechnung“; mit einem Platz hätte jeder Formatwechsel neu gerechnet.
- **P11 Ersteller:** Der Ersteller wird von Leerraum befreit (` Erika ` wird `Erika`); Auto, leer oder nur Leerraum ergibt keinen Ersteller, nie den Anmeldenamen (Antwort auf Q2).
- **P12 Alle Blätter:** Jedes Blatt wird in einem eigenen Thread mit eigenem Fortschritt gerechnet; der Gesamtbalken wächst über `set_total`/`advance` (öffentliche Schnittstelle, `Progress` bleibt unverändert). Es gibt keinen Zwischenspeicher zwischen den Blättern und zum Zeichnungsbereich (siehe Grenzen). Der Schlüssel des Ergebnisses ist jetzt eine Struktur mit Ziel (`One(Id)` oder `All`), Änderungszahl, Format, Rahmen und Dokumentname; bei „Alle Blätter“ steckt keine Zeichnungs-Id im Schlüssel, eine andere Wahl in der Auswahl rechnet also nicht neu.
- **P14 Hilfsdatei:** `write_via_temp` hat einen vierten Parameter (einen Haken, der nach jedem geschriebenen Stück und vor dem Umbenennen läuft), und es gibt `export_pdf_file_then` und `export_drawing_file_then`; die öffentlichen Funktionen sind dünne Hüllen. Grund: Die Tests müssen über genau denselben Weg laufen wie der Export.
- **Antworten auf die Fragen des Plans:** Q1: Stauchen, aber nur gegen die Zelle (P6). Q2: Nur ein selbst eingetragener Ersteller (P11).
- Sonst wie geplant: P1 (Dateiaufbau), P2 (Zahlen), P4 (Strichbreiten), P7 (Eigenschaften; `pdf_date` prüft zusätzlich Tag gegen Monat und Schaltjahre), P8 (Grenzen), P10 (Dateiname und Titel), P13 (Zeichenprüfung mit denselben Feldnamen wie DXF; die DXF-Meldungen sind wörtlich unverändert). Kleine Anpassungen fremder Tests: eine Zeile in `tests/export_drawing.rs` (erschöpfendes `match` über `DrawingFormat`), die Formatzahl 8 in `export_drawing.rs` und `export_step.rs`, und `fit_width: None` in Mustern der Zeichnungstests.

### Grenzen
- Die Schrift ist nicht eingebettet. Jeder Betrachter ersetzt Helvetica durch seine eigene Schrift (Arial, Helvetica oder ähnlich); die Darstellung ist je Betrachter leicht anders. Die Breiten für Zentrieren und Rechtsbündigsetzen stammen aus den Adobe-Daten (Lizenzvermerk: README, „Drittanbieter“, und Kopf von `pdf_font.rs`).
- Nur der westeuropäische Zeichensatz (WinAnsi, 219 Zeichen). Emoji, chinesische, kyrillische und andere Zeichen sperren den Export mit einer Meldung, die Zeichen und Feld nennt (bei „Alle Blätter“ mit dem Blatt). Das Minuszeichen U+2212 wird als „-“ geschrieben.
- Keine Komprimierung (PDF 1.4, Inhaltsströme im Klartext); die Dateien sind größer als nötig. Keine Einbettung, keine Lesezeichen, kein PDF/A, keine Verschlüsselung, keine Farben (schwarz; verlorene Maße rot, wegen der Sperre in der Praxis nie ausgegeben).
- „Alle Blätter“ rechnet jedes Blatt von Grund auf neu, auch wenn mehrere Blätter dieselben Ansichtsarten derselben Körper zeigen, und jede Änderung am Dokument rechnet alle Blätter neu. Bei vielen Blättern ist das langsam. „Exportieren …“ ist erst frei, wenn alle Blätter fertig sind.
- Höchstens 200 Seiten je PDF (mehr: Meldung vor dem Rechnen, einzelne Blätter bleiben möglich) und höchstens 14 400 pt je Seitenkante (mit A4 bis A0 unerreichbar, nur im Schreiber geprüft).
- Datum in den Eigenschaften nach UTC (ein gültiges Ausgabedatum des Blatts hat Vorrang), Ersteller nur, wenn ausdrücklich eingegeben.
- Stauchung breiter Zeichen in Zellen (P6); sehr spitze Ecken können sich vom SVG unterscheiden (P3).
- Die Golden-PDFs hängen an `sin` und `cos` der Plattform: Die Zahlen werden auf vier Nachkommastellen (pt) gerundet, und an einer Rundungsgrenze kann ein Unterschied im letzten Bit eine Ziffer kippen. Das ist selten; auf diesem Rechner (Windows) stabil. Wird `golden_files` bei der CI unter Linux oder macOS rot, ist das ein möglicher, harmloser Befund: die Golden-Dateien dort einmal mit `PLANORA_UPDATE_PDF_GOLDEN=1` neu erzeugen und prüfen, dass nur einzelne Ziffern abweichen.
- Der Test im echten Fenster (Controller) und die Handliste unten sind noch offen.
- Poppler meldet auf diesem Rechner „No display font for …“, solange keine Ersatzschriften eingerichtet sind; das liegt an der Einrichtung, nicht an der Datei.

### Zahlen
- Zeichensatz: 219 zulässige Zeichen, 224 Breiten (0x20 bis 0xFF), alle gegen eine Referenztabelle im Test geprüft; die 315 Glyphbreiten der Adobe-Quelle sind in zwei unabhängigen Abschriften identisch.
- Tests: `cargo test --workspace -j 2 --no-fail-fast` am Ende der Aufgabe 6: 279 Testläufe, 2616 Tests bestanden, 0 fehlgeschlagen, 38 ausgeschaltet; davon in der App 856 Tests (`export_pdf` 40). `cargo fmt`, `cargo clippy -D warnings` und `cargo deny check` grün.
- Mutationsproben: rund 185 einzelne Mutationen über die Aufgaben 1 bis 5 (Berichte unter `.superpowers/sdd/2026-10-18-pdf-ausgabe/`), jede rot; Überlebende wurden durch schärfere Tests ersetzt.
- Dateigrößen der Proben: 4 KB (A4 quer ohne Rahmen) bis 23 KB (A0 quer, 12 Zeilen, Rahmen); `alle-blaetter.pdf` mit 6 Seiten 101 KB. Ein Test rechnet 200 Blätter und schreibt eine echte PDF mit 200 Seiten.

### Handliste für den Nutzer (Prüfung von Hand, Stand 2026-10-19)
Proben liegen in `D:\Testumgebung\zeichnung-proben\pdf` (Namen siehe oben). Bitte jeweils in Edge, Chrome und einem weiteren Betrachter (zum Beispiel SumatraPDF oder Adobe Acrobat) öffnen.

**A. Proben ansehen**
1. `Blatt_A4hoch_Rahmen.pdf`, `Blatt_A4quer_Rahmen.pdf`, `Blatt_A3quer_Rahmen.pdf`, `Blatt_A0quer_Rahmen.pdf`: Die Seite hat die richtige Größe und Ausrichtung (hoch oder quer), der Rahmen mit Raster und Zentriermarken ist vollständig, nichts ist abgeschnitten.
2. Im Schriftfeld und in den Zellen stehen die Texte vollständig in ihren Zellen; „Ø“, „°“, „±“, „µ“ und „€“ erscheinen richtig (besonders in den Proben `Stueckliste_…_lang_…`), Umlaute und „ß“ ebenso. In den Längstexten endet ein gekürzter Text mit „…“ und ragt nicht über die Zelle.
3. Linien: Sichtbare Kanten sind kräftig, verdeckte Kanten gestrichelt und dünner, Mittellinien Strich-Punkt, Maßlinien dünn mit gefüllten Pfeilen. Die Stärken sehen aus wie in der danebenliegenden SVG-Datei (beide nebeneinander im Browser öffnen und vergleichen). Der Kreis in der Vorderansicht ist rund, die Ellipsen in der Isometrie sind glatt.
4. Maße: „40“ unter der Vorderansicht, „20 -0,1“ rechts und senkrecht gedreht, „Ø 10 ± 0,1“ mit Hinweislinie. Das Minus steht als Strich.
5. `…_ohneRahmen.pdf`: nur Ansichten, Maße und Mittellinien (bei `Stueckliste_…` dazu die Nummernkreise 1, 12 und 200, aber keine Tabelle). Die „200“ passt in den Kreis und ist kleiner als die „1“.
6. `Stuecklistenblatt_Rahmen.pdf` (zwei Seiten) und `Stuecklistenblatt_ohneRahmen.pdf` (nur die Tabelle): Seite 1 hat 44 Zeilen, Seite 2 eine Zeile; nichts überlappt.
7. `alle-blaetter.pdf`: sechs Seiten in der Reihenfolge A4 hoch, A3 quer, Stücklistenblatt (zwei Seiten), A0 quer, A4 quer, jede in ihrer eigenen Größe.
8. Drucken: A0- und A3-Proben im Betrachter auf A4 verkleinert drucken („An Seite anpassen“): alles gleichmäßig kleiner, nichts springt.

**B. Export in der Oberfläche**
9. Projekt mit einem Quader und einer Zeichnung: „Datei“ → „Exportieren …“, Format „Zeichnung als PDF“. Die Karten stehen gleich hoch nebeneinander, die Wahl „Gewähltes Blatt“ / „Alle Blätter“ ist da (Vorgabe: gewähltes Blatt). Exportieren und die Datei in den drei Betrachtern öffnen: eine Seite, der Titel in den Dokumenteigenschaften ist der Name der Zeichnung.
10. „Blattrahmen mit ausgeben“ ausschalten und neu exportieren (das Fenster rechnet kurz neu): kein Rahmen, kein Schriftfeld, aber Ansichten und Maße. Wieder einschalten: Der Rahmen ist da.
11. Dokument mit drei Zeichnungen, darunter ein Stücklistenblatt mit Folgeseite (45 Körper, A4 hoch): „Alle Blätter“ wählen. Die Zeichnungsauswahl ist gesperrt, mit Hinweis; das Fenster nennt „Blätter: n“, beim Rechnen steht „Blatt 2 von 5 wird berechnet …“. Die PDF hat so viele Seiten wie Blätter, in der Reihenfolge des Browsers; „Blatt n von N“ stimmt auf jeder Seite, Pos. 45 steht nur auf dem Folgeblatt.
12. Ein Gesichtszeichen (Emoji) in den Titel eintragen (Fenster „Blattangaben“), Export als PDF: Meldung „Das Zeichen „…“ im Feld „Titel“ lässt sich in einer PDF nicht ausgeben.“, keine Datei, keine Hilfsdatei im Ordner. Bei „Alle Blätter“ steht „Im Blatt „…“: “ davor. SVG geht weiter. Emoji entfernen: PDF geht. Der Titel „Platte −0,1 Ø ± ° µ €“ geht dagegen und steht mit „-“ in der PDF.
13. Eine Positionsnummer setzen und das Teil löschen (Kreis wird rot mit „?“): Der PDF-Export meldet die Ansicht und schreibt nichts. Bei „Alle Blätter“ kommt der Blattname dazu, und ein kaputtes Blatt mitten im Dokument sperrt die ganze PDF (kein Blatt wird weggelassen). Kreis löschen: PDF geht.
14. Dateiname: „test“ ergibt „test.pdf“, „test.PDF“ wird „test.pdf“, „a/b?c“ wird „a_b_c“ (unzulässige Zeichen werden zu „_“).
15. Abbrechen: Bei „Alle Blätter“ während der Berechnung auf „Abbrechen“ klicken: „Abgebrochen“, keine Datei. Mit einer vorher angelegten Zieldatei gleichen Namens: Sie bleibt unverändert.
16. Tab schließen, während das Fenster rechnet oder schreibt: Das Programm bleibt bedienbar, im Zielordner bleibt keine Hilfsdatei „.…planora-tmp“ zurück.
17. Fenster bei 800 × 600 und groß, jeweils Hell und Dunkel: Nichts ragt aus dem Fenster, die Karten sind gleich hoch, Wahl und Hinweis sind lesbar.
18. SVG und DXF als Gegenprobe exportieren: Beide sind wie vorher.

## Teilprojekt 3d – Schnittansichten

### Stand und Geliefertes
Stand: 2026-10-19 (Zweig `feature/schnittansichten`, Basis `master` @ `7f1e319`). Zeichnungen können Vollschnitte als abhängige Ansichten haben: Schnittlinie mit zwei Klicks in einer geraden Ansicht, Körper mit dem vorhandenen Kernel geschnitten, Schnittfläche schraffiert (Löcher frei), Schnittlinie mit Pfeilen und Buchstaben in der Ausgangsansicht, Titel „A–A (1:2)“ unter der Schnittansicht, Ausgabe in SVG, DXF und PDF, Maße und Positionsnummern in der Schnittansicht, Dateiformat 8 nur mit Schnitten. Bedienung und Grenzen für Anwender: README, Abschnitt „Schnittansichten“. Entwurf: `docs/superpowers/specs/2026-10-19-schnittansichten-design.md` (Festlegungen der Umsetzung in §11), Plan: `docs/superpowers/plans/2026-10-19-schnittansichten.md` (S1 bis S12, „Abweichungen bei der Umsetzung“).

- `planora-drawing`: `section.rs` (Schneidekörper, `cut_body`, Schnittflächen an der Herkunftsmarke `u64::MAX − Körperindex`, Schleifen und Vielecke, `compute_section`), `hatch.rs` (Zuschnitt paralleler Linien gegen Vielecke nach gerade-ungerade, rein 2D, `MAX_HATCH_LINES` 20 000), `section_marks.rs` (Schraffur, Schnittlinie, Titel als Markierungen), Modell `ViewSpec.section` mit `SectionSpec`/`HatchSpec`/`ViewKey` und `Drawing::{add_section, next_letter}`, Dateiformat 8, zwei neue Markenarten `Hatch` und `SectionLine(Chain|End)`, Ausgabe in den drei Schreibern (Gruppen und Ebenen „Schraffur“, „Schnittlinie“, „Schnittlinie dick“), eigene Pfeilgröße.
- `planora-app`: Schnittansichten im Hintergrund-Zwischenspeicher (Schlüssel `ViewKey`, Verdrängung `MAX_CACHED`), Schraffur je Blatt neu gerechnet, Werkzeug „Schnitt“ mit Vorschau, Einrasten und Ansichtsfeld (Schnittposition, Umdrehen, Schraffur, Abstand), Löschen mit Rückfrage, Schnittansicht im Browser, Meldungen, Export-Sperren, Maße und Positionsnummern auf den geschnittenen Körpern.
- `planora-feature`: Auflösung der Maße gegen die Körper der Ansicht (`resolve_drawing_dimensions_views`, `ResolveFn` mit `&ViewSolids`).
- Kernel: eine kleine Korrektur in `crates/planora-kernel/src/boolean/seg.rs` (`Seg::closest`), auf Wunsch des Nutzers (siehe Aufgabe 12a). Netz, STEP, Austausch, Abhängigkeiten, `deny.toml`: unverändert.
- Proben: `crates/planora-drawing/tests/hand_samples_section.rs` (ausgeschaltet; Ziel `D:\Testumgebung\zeichnung-proben\schnitt`): sechs Proben je SVG, DXF und PDF, je ohne und mit Rahmen (36 Dateien) und `ERWARTUNG.txt`.

### Festlegungen S1 bis S12 mit Abweichungen
Die Festlegungen stehen im Plan (Kopf) und kurz in der Spec §11. Was beim Bauen anders wurde oder zusätzlich festgelegt wurde:
- **Aufgabe 1:** Die Prüfhilfe tastet gekrümmte Kanten mit 4096 statt 512 Punkten ab (bei 512 fehlen dem Rohr durch die Vieleck-Näherung etwa 5e-3 an der Fläche, vorgegeben waren ±1e-3). Der Schneidekörper der Nut im Test ragt bis z = 12 statt bündig bis z = 10 (selber Körper, vermeidet im Testaufbau deckungsgleiche Flächen). Kernel unverändert; alle Grundfälle gingen ohne Rückfall.
- **Aufgabe 2 (Grenzen):** Kreise und Ellipsen höchstens 65 536 Stücke, Freiformkanten höchstens 2049 Punkte je Kante (der Auftrag nannte 4096). Wird eine Grenze erreicht, wird das Vieleck gröber als die Toleranz, ohne Meldung. Das ist praktisch nicht erreichbar (Kreisgrenze erst ab etwa 870 m Radius bei 1e-6 mm Toleranz; die Freiformkante müsste extrem wellig sein).
- **Aufgabe 3:** Die Zahl in `HatchError::TooMany(n)` ist nur beim Linienzählen die echte Zahl, beim Bauen eine Untergrenze (`max + 1`). Sie wird nirgends angezeigt (auch nie in Meldungen), nur „zu fein“. Eine Linie genau auf der Unter- oder Linkskante der Fläche zeichnet ein Stück auf der Kontur (zählt als doppelte Konturlinie, harmlos).
- **Aufgabe 4:** Eine Normansicht unter einer Schnittansicht bleibt im Modell erlaubt (das Modell ist dort freizügig); die Oberfläche bietet „Ansicht hinzufügen“ mit einer Schnittansicht als Eltern nicht an. Schnitt aus Schnittansicht ist im Modell und in der Datei verboten. Der Schnittname trägt U+2013 („Schnitt A–A“).
- **Aufgabe 6:** Zusätzliche Meldung „Die Schraffur von „Schnitt A–A“ lässt sich nicht berechnen.“ (nur bei ungültigem Abstand; über die Oberfläche praktisch nicht erreichbar). Die Schnittlinie liegt auf der Hüllbox der Ausgangsansicht ohne Mittellinien.
- **Aufgabe 7:** Der Schnittpfeil ist auf A4 5·f = 3,54 mm lang (Blattfaktor, S8), nicht 5 mm; die Tests prüfen A3 mit 5 und A4/A0 mit 5·f. Die App-Zeichnung selbst (Stift der Enden, Kette, Pfeilgröße in `drawing_view.rs`) hat keinen Test, der sie festhält; sie ist über die Handliste abzudecken.
- **Aufgabe 8:** Über den Auftrag hinaus ein Verdrängen im Zwischenspeicher: höchstens `MAX_CACHED = 7 + MAX_SECTIONS` fertige Einträge (gebrauchte bleiben immer, laufende ungebrauchte fallen weg), sonst wüchse die Zahl beim Ziehen der Ebene ohne Ende. Das Verdrängen folgt der Einfügereihenfolge, nicht der letzten Benutzung: Wechsel zwischen zwei Blättern mit zusammen über 19 Schlüsseln rechnet einen Teil neu (nur Leistung). `dimension_state` fragt schon nach `result_of`.
- **Aufgabe 9:** Der Knopf „Blattangaben“ im Werkzeugfeld zeigt nur noch sein Symbol (Hinweis „Blattangaben: …“, Menüeintrag bleibt), damit „Schnitt“ bei 1100 × 700 in die Zeile passt (Vorbild „Blatt einpassen“). Wählt man eine Schnittansicht an, geht das Werkzeug „Schnitt“ aus. Ein Klick auf die Schnittlinie (6 Pixel) gewinnt gegen die Kanten der Ausgangsansicht. **Das Winkelfeld der Spec §4 fehlt absichtlich** (S7: Winkel nach Rang des Körpers, 45° und 135°); im Ansichtsfeld stehen Position, Umdrehen, Schraffur und Abstand. Die Rückfrage beim Löschen nennt Maße (nach der Gesamtprüfung: „Die Schnittansicht „Schnitt A–A“ wird gelöscht. Mit ihr verschwinden die Schnittlinie in „Vorne“ und 2 Maße.“).
- **Aufgabe 10:** Die Schnittmarke einer Schnittfläche ist `u64::MAX − i` mit `i` dem Index des Körpers unter den exakten Körpern: Maße an Schnittkanten hängen an der Körperreihenfolge. Löscht man einen früheren Körper (oder wird ein Netz-Körper exakt), wird so ein Maß rot und der Export gesperrt, nie ein falscher Wert. „Maße vorschlagen“ entfernt Doppelte über die ganze Zeichnung (die Schlüssel sind geometrisch): Eine Schnittansicht bekommt kein Maß, das eine andere Ansicht schon trägt (zum Beispiel die Umrissbreite). `Document::resolve_dimensions`, `resolve_drawing_dimensions` und `_keyed` lassen Maße in Schnittansichten ganz weg (im Doc-Kommentar vermerkt); für Zeichnungen mit Schnitten gilt `resolve_drawing_dimensions_views`. `ResolveFn` hat einen zusätzlichen Parameter `&ViewSolids` (nur Test-Hilfen betroffen). Im Export hat ein Maß in einer Schnittansicht eine Nachkommastelle („8,0“). **Spec-Lücke:** Eine Positionsnummer in einer Schnittansicht behält ihre Nummer, wenn ihr Teil durch eine verschobene Ebene nicht mehr im Schnitt vorkommt; der Kreis zeigt dann ins Leere, der Export läuft. Mögliche Regel für später: als verloren zeigen, wenn der Körper nicht in `SectionData.source` der Ansicht steht.
- **Aufgabe 11 (neu, aus den Proben):** Der Kernel versagte bei manchen Ebenen, die nahe an einer Bohrung liegen. Behoben in Aufgabe 12a.
- **Aufgabe 12a (Kernel-Korrektur, vom Nutzer freigegeben):** Die Vorgabe des Plans „Kernel unverändert“ wurde für genau diese eine Änderung aufgehoben; Dennis hat „Kernel reparieren“ gewählt. Ursache: Eine Bohrung hat als Rand einen ganzen Kreis aus einer einzigen Kante, Anfang und Ende sind derselbe Punkt (die Naht). Die Suche nach dem nächsten Punkt auf so einem Kreis schaute bei einem Punkt kurz vor dem Ende (innerhalb von 3,75°) nur am Anfang nach und fand dort nur die Naht. Der Kreis wurde dann an der Schnittstelle nicht geteilt, und der Schnitt scheiterte. Die Naht liegt bei Bohrungen genau in der Ebene durch die Achse, also traf es gerade den häufigsten Schnitt „durch die Mitte“ und Ebenen bis etwa 0,065 × Radius daneben. Jetzt sucht der Kernel bei geschlossenen Kreisen und Ellipsen an beiden Enden und nimmt den näheren Punkt. Freiformkurven bleiben bewusst bei der alten Suche (zwei vorhandene Tests erwarten dort weiter die benannte Ablehnung). Die Korrektur hilft allen booleschen Operationen, nicht nur dem Schnitt. Tests: `crates/planora-kernel/tests/seg_closest.rs` (Kreise, Ellipsen, offene Bögen, Freiform, Quader mit Bohrung) und die Gefahrenzonen-Matrix in `stage28_section_cut.rs`. Die Probe `DreiSchnitte_A0quer` schneidet wieder bei z = 100 durch die Bohrungsachse. Untersuchung: `.superpowers/sdd/2026-10-19-schnittansichten/bore-cut-investigation.md`.
- **Nachbesserung nach der Gesamtprüfung:** (1) Liegt die Ebene nur 5e-5 bis 1e-3 mm neben einer Bohrungsachse, ging der Schnitt zwar, aber die Ansicht konnte nicht sicher sagen, was verdeckt ist (Schnittkante und Umriss der Bohrung fast deckungsgleich; 412 von 1728 Fällen der Prüfung). Jetzt rechnet das Programm dann einmal mit der um 0,001 mm zur behaltenen Seite verschobenen Ebene und zeigt den Hinweis wie im Tangentialfall; das gilt nur, wenn dabei dieselben Körper Schnittflächen haben; sonst, und wenn das Schneiden an der verschobenen Ebene scheitert (etwa weil ein knapp gestreifter Körper dort ganz wegfiele), bleibt genau diese erste Meldung, nie „trifft den Körper nicht“ oder „lässt sich der Schnitt nicht berechnen“. Nachgemessen: 480 von 480 Fällen (Quader 400 × 300 × 200, Bohrung r 4 bis 75 in allen Richtungen, 5e-5 bis 1e-3 mm neben der Achse) ergeben eine Ansicht, vorher 324. (2) Die Hüllbox, nach der ein Körper ohne Kernel ganz behalten oder ganz entfernt wird, war bei runden Kanten bis etwa 1,2e-3 × Radius zu klein (64 Stützpunkte je Kante), bei Kugeln sogar ganz falsch (ihr Äußerstes liegt auf keiner Kante). Eine Ebene knapp innerhalb eines großen Zylinders oder einer Kugel ließ den Körper deshalb still verschwinden oder ungeschnitten. Jetzt ist die Box sicher: Kreise und Ellipsen genau, Kugel, Torus und Freiformflächen über eine sichere Box, Freiformkanten über ihre Kontrollpunkte. (3) Wortlaut der Rückfrage beim Löschen einer Schnittansicht. (4) Kernel-Kommentar: Bei einem Punkt genau auf der Naht kann `Seg::closest` jetzt f ≈ 1 statt f ≈ 0 liefern (derselbe Punkt).

### Grenzen
- Nur Vollschnitt an Hauptebenen. Nicht dabei: Halbschnitt, Teilschnitt (Ausbruch), Stufen- und Winkelschnitt, Detailansicht, Schnitt aus Schnittansicht und aus Isometrie. Höchstens 12 Schnitte je Blatt (Buchstaben ohne I, O, Q).
- Netz-Körper werden nicht geschnitten und nicht gezeichnet (wie bisher in Ansichten, mit Meldung).
- Rippen und Wellen werden schraffiert (Norm-Sonderfall nicht umgesetzt). Keine Werkstoffmuster. Schnittlinie, Schraffur, Pfeile und Beschriftung folgen gängiger Praxis: **die Normen ISO 128-40, -44, -50 und DIN 6 liegen nicht vor, die Werte sind unbelegt.**
- Die Schraffur wird je Blatt neu gerechnet (Abstand 3 mm × Blattfaktor, nicht abhängig vom Maßstab), nicht gespeichert und nicht zwischen Blättern geteilt. Höchstens 20 000 Schraffurstücke je Ansicht („zu fein“), Abstand auf dem Blatt mindestens 0,5 mm; ein Blattwechsel, der darunter fiele, wird abgelehnt.
- Die Buchstaben an den Pfeilen haben die Höhe des Maßtexts (3,5 mm) auf jedem Blatt; auf A0 wirken sie neben dem Titel (14 mm) klein. Das folgt dem Plan (S8), ist aber auf großen Blättern unschön.
- Maße an Schnittflächen hängen an der Körperreihenfolge (siehe Aufgabe 10). Positionsnummern in Schnittansichten werden nicht „verloren“, wenn das Teil aus dem Schnitt fällt (Spec-Lücke).
- Kreise und Ellipsen über 65 536 Stücke, Freiformkanten über 2049 Punkte: gröbere Vielecke ohne Meldung, praktisch unerreichbar.
- **Kernel nahe der Bohrungsnaht (behoben in Aufgabe 12a):** Früher scheiterte der Schnitt, wenn die Ebene genau durch die Achse einer Bohrung ging oder knapp daneben lag (bis etwa 0,065 × Radius, zum Beispiel Radius 10 bis 0,65 mm, Radius 40 bis 2,6 mm). Nach der Korrektur im Kernel gelingen alle gemessenen Fälle mit der richtigen Fläche (Quader mit Bohrung Radius 4, 10, 12 längs z, Radius 10 längs x und y, großer Quader mit Radius 40, Platte mit zwei Bohrungen; Abstände 0 bis ±1 mm). Nicht eigens geprüft: eine Ebene, die eine Bohrung fast nur streift und deren Naht genau dort liegt; die Meldung „… lässt sich der Schnitt nicht berechnen.“ bleibt die Absicherung, nie ein falsches Bild. Ebenen 5e-5 bis 1e-3 mm neben der Achse werden seit der Nachbesserung um 0,001 mm verschoben (Hinweis im Ansichtsfeld).
- **Bohrungen mit gerundeter Kante:** Der Kernel kann einen Torus (die Rundung) noch nicht mit einer Ebene schneiden, die durch ihn geht. Ein Längsschnitt durch eine Bohrung mit gerundeter Kante gibt daher „… lässt sich der Schnitt nicht berechnen.“ (nachgemessen: Quader 100 × 60 × 40, Bohrung r 10, obere Kante mit r 2 gerundet; Ebenen y = 30, 30,5, 33, 41 scheitern, Querschnitte z = 39 und 20 und x = 50 gehen). Querbohrungen (Zylinder in Zylinder) kann der Kernel gar nicht erst bauen, sie kommen im Schnitt also nicht vor.
- **Streifschnitt an großen runden Körpern:** Liegt die Ebene nur etwa 0,001 mm innerhalb des Rands eines großen Zylinders (r 100), kann die Ansicht melden, dass sie nicht sicher sagen kann, was verdeckt ist. Bei 0,01 mm geht es. Nie verschwindet der Körper still.
- Poppler meldet auf diesem Rechner „No display font for …“, solange keine Ersatzschriften eingerichtet sind; das liegt an der Einrichtung, nicht an der Datei.
- Die Handprüfung (Handliste unten) ist noch offen: Inkscape, Fusion (DXF) und zwei PDF-Betrachter haben die Proben noch nicht gesehen; der Test im echten Fenster (Controller, Aufgabe 12) steht aus.

### Zahlen
- Tests: `cargo test --workspace -j 2 --no-fail-fast` am Ende der Aufgabe 11 (vor der Kernel-Korrektur; Stand nach Aufgabe 12a im Bericht `task-12a-report.md`): 291 Testläufe, 2764 Tests bestanden, 0 fehlgeschlagen, 39 ausgeschaltet (darunter die Probenerzeugung `hand_samples_section`). `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` und `cargo deny check` grün (nur die bekannten Warnungen zu doppelten Versionen). Kernel, Netz, STEP, Austausch, `Cargo.toml`, `Cargo.lock` und `deny.toml` ohne Diff gegen `7f1e319` (danach in Aufgabe 12a nur `boolean/seg.rs` im Kernel geändert, dazu eine Testdatei).
- Mutationsproben: 189 einzelne Mutationen aus den Berichten der Aufgaben 1 bis 10 (`.superpowers/sdd/2026-10-19-schnittansichten/`, Summenliste in `task-11-report.md`), jede rot außer sechs begründet überlebenden (gleichwertig, Absicherung oder Testlücke); dazu die eigenen Proben der Prüfer (rund 280).
- Proben: 36 Dateien, 4 bis etwa 60 KB je Datei; alle PDFs mit Poppler (`pdfinfo`, `pdftotext`, `pdftoppm`) geprüft und sechs Seiten angesehen (Schraffur um Löcher frei, Schnittlinie, Pfeile, Buchstaben, Titel).

### Handliste für den Nutzer (Prüfung von Hand, Stand 2026-10-19)
Proben liegen in `D:\Testumgebung\zeichnung-proben\schnitt` (Namen: `Schnitt_<Probe>_<ohneRahmen|Rahmen>.<svg|dxf|pdf>`; Proben `RohrQuer_A4quer`, `RohrLaengs_A4quer`, `Quader_A3quer`, `PlatteBolzen_A4quer`, `Stufe_A4quer`, `DreiSchnitte_A0quer`). Neu erzeugen: `cargo test --release -p planora-drawing --test hand_samples_section -- --ignored`.

**A. Proben ansehen** (SVG in Inkscape, DXF in Fusion, PDF in zwei Betrachtern, zum Beispiel Edge und SumatraPDF)
1. `RohrQuer_A4quer`: unten ein Ring, schraffiert, die Bohrung in der Mitte ist frei. Oben die Vorderansicht mit der Schnittlinie A–A: dünne Strich-Punkt-Linie, die Enden dick und kurz abgewinkelt, an beiden Enden ein Pfeil nach unten und der Buchstabe „A“. Unter dem Ring steht „A–A (1:1)“.
2. `RohrLaengs_A4quer`: das Rohr längs durch die Achse: zwei Wandstreifen schraffiert, dazwischen frei; die Schnittansicht liegt rechts von der Vorderansicht, die Pfeile zeigen nach rechts.
3. `Quader_A3quer` (Maßstab 1:2): Schnitt A zeigt zwei senkrechte freie Streifen (die Bohrungen) in der Schraffur; Schnitt B zeigt die Nut oben als Einschnitt. Um Schnitt A stehen die Maße „100,0“ und „60,0“ mit Pfeilen. Titel „A–A (1:2)“ und „B–B (1:2)“.
4. `PlatteBolzen_A4quer` (2:1): Platte und Bolzen sind in verschiedene Richtungen schraffiert (45° und 135°), zwischen Bolzen und Bohrung bleibt ein heller Spalt, keine Schraffurlinie läuft über ihn.
5. `Stufe_A4quer`: Schnitt A liegt genau auf der Stufenfläche (das Programm verschiebt ihn um 0,001 mm); er zeigt das untere Rechteck ganz schraffiert. Schnitt B quer dazu.
6. `DreiSchnitte_A0quer`: die Buchstaben A, B und C, jeder Schnitt mit eigener Linie, eigenen Pfeilen und eigenem Titel. Schnitt A geht genau durch die Mitte der Bohrung: Der freie Streifen ist so breit wie die Bohrung (80 mm, auf dem Blatt 40 mm), etwas breiter als bei C. Die Buchstaben sind klein (Maßtexthöhe), die Titel groß.
7. Linienstärken: Schraffur dünn, Kontur der Schnittfläche kräftig, Schnittlinie Strich-Punkt mit dicken Enden. SVG, DXF (Ebenen „SCHRAFFUR“, „SCHNITTLINIE“, „SCHNITTLINIE_DICK“ in Fusion sichtbar) und PDF sehen gleich aus. Die Dateien ohne Rahmen zeigen nur Ansichten; mit Rahmen kommen Rahmen und Schriftfeld dazu und nichts überlappt.
8. Der Halbgeviertstrich in „A–A“ erscheint in allen drei Formaten richtig (in der DXF-Datei in Fusion besonders prüfen).

**B. In der Oberfläche** (Release-Build, ein Quader mit Bohrung und eine Zeichnung)
9. In „Vorne“ eine waagerechte und danach eine senkrechte Schnittlinie mit zwei Klicks ziehen: Die Linie rastet auf die Mitte ein, die Vorschau zeigt Pfeile; die Schnittansicht erscheint in Pfeilrichtung, Löcher bleiben frei, die Buchstaben A und B stimmen; „Schnitt“ ist danach wieder aus.
10. Schnittansicht wählen: „Schnittposition“ eintippen (Ansicht und Linie folgen), „Blickrichtung umdrehen“ (Pfeile und Ansicht wechseln die Seite), „Schraffur“ aus und an, „Schraffurabstand“ ändern. Der Maßstab ist gesperrt; ändert man den Maßstab der Ausgangsansicht, zieht die Schnittansicht mit.
11. Modell ändern (Bohrung löschen): Die Schnittansicht folgt, die Bohrung ist weg.
12. Schnittposition weit hinter das Teil schieben: Meldung „Die Schnittebene trifft den Körper nicht.“, Export gesperrt; zurückschieben: Ansicht kommt wieder.
13. Schnittansicht löschen: Rückfrage, die Schnittlinie verschwindet mit. Dasselbe von der Schnittlinie aus. Der Buchstabe wird beim nächsten Schnitt wieder vergeben.
14. In der Schnittansicht ein Maß setzen und „Maße vorschlagen“: Das Maß hängt an der Schnittkante; beim Ändern der Position folgt es oder wird rot (nie ein falscher Wert).
15. Speichern und wieder laden: Schnitt, Schraffur und Maße sind da. Ein Dokument ohne Schnitt bleibt im alten Format.
16. Schnitt durch die Mitte einer größeren Bohrung legen und die Schnittposition danach auf wenige Zehntausendstel daneben ändern (zum Beispiel 150,0003): Der Schnitt gelingt jedes Mal, die Schraffur lässt die Bohrung frei; bei der winzigen Abweichung steht im Ansichtsfeld der Hinweis, dass die Ebene um 0,001 mm verschoben wurde. Eine Bohrung mit gerundeter Kante der Länge nach schneiden: Es kommt die Meldung „… lässt sich der Schnitt nicht berechnen.“, die Ausgabe ist gesperrt.
17. Hell und Dunkel, Fenster 1100 × 700 und 800 × 600: Der Knopf „Schnitt“ passt in die Zeile, „Blattangaben“ zeigt nur sein Symbol mit Hinweis, nichts ragt heraus.
18. SVG, DXF und PDF aus der Oberfläche exportieren und mit den Proben vergleichen.

## Teilprojekt: Rückgängig und Wiederholen

### Stand und Geliefertes
Stand: 2026-10-20 (Zweig `feature/rueckgaengig`, Basis `master` @ `6685f3e`). Jede Änderung am Dokument lässt sich rückgängig machen und wiederholen, je Tab, mit dem Namen der Aktion.

- **Bedienung:** Strg+Z (Rückgängig), Strg+Umschalt+Z und Strg+Y (Wiederholen); im Menü „Bearbeiten“ ganz oben „Rückgängig: <Aktion>“ und „Wiederholen: <Aktion>“ mit dem Kürzel; **zwei Symbolknöpfe im oberen Feld** gleich neben den Menüs (in 3D, Zeichnung und Skizze). Nicht im Befehlsfeld unten, siehe Abweichungen.
- **Verlauf je Tab:** höchstens 100 Schritte und etwa 200 MB (geschätzt, Netze zählen nicht), die ältesten fallen still heraus. Eine neue Aktion nach Rückgängig leert die Wiederholen-Liste.
- **Schritte:** Ein Schritt ist, was zwischen zwei Bildern ohne gedrückte Maustaste und ohne Tippen im Feld geschieht. Ziehen (Position, Ansicht, Maßlinie, Positionsnummer, Skizzenpunkt) und Tippen einer Zahl sind je ein Schritt; „Löschen mit Folgen“ ist ein Schritt. Gibt es keinen Unterschied zum Stand davor, entsteht kein Schritt.
- **Skizze:** Eigener Skizzenverlauf, solange die Skizze offen ist; das Dokument ruht. Der erste Klick eines Werkzeugs ist kein Schritt; hat ein Werkzeug einen offenen Punkt, nimmt das erste Rückgängig nur diesen Punkt zurück. „Skizze beenden“ ist danach ein Dokumentschritt („Skizze bearbeiten“), der die ganze Skizze zurücknimmt.
- **Punkt „ungespeichert“:** folgt dem Verlauf (Schrittnummern, nicht dem Änderungszähler). Rückgängig zum gespeicherten Stand: kein Punkt; davon weg: Punkt. Rückgängig geht auch nach dem Speichern. Ein gespeicherter Stand, der aus dem Verlauf herausgefallen ist, bleibt „ungespeichert“ bis zum nächsten Speichern.
- **Autosicherung:** Ist ein Tab durch Rückgängig wieder gleich dem gespeicherten Stand, wird seine eigene Autosicherung gelöscht (auch beim Schließen), sonst bot das Programm sie beim Öffnen als überholt an. Wiederholen schreibt sie neu.
- **Auswahl nach Rückgängig:** Der gewählte Körper bleibt gewählt und folgt dem Körper durch Verschieben, Verrunden und Boolesche Operationen (vorwärts und rückwärts); gibt es ihn nicht mehr, wird er abgewählt. Flächen- und Kantenwahl und Hervorhebung werden beim vollen Zurückladen geleert; bei Schritten nur an der Zeichnung bleiben sie. Kamera und Hell/Dunkel bleiben immer.
- **Fenster:** Nach jedem gelungenen Rückgängig/Wiederholen schließen alle offenen Fenster und Rückfragen des Tabs (sie halten Kennungen des alten Standes). Ein Export, der gerade schreibt, wird nicht geschlossen (dann ist Rückgängig gesperrt).
- **Tastenschutz:** Strg+Z und Strg+Y wirken nicht, wenn getippt wird (dort gilt das Rückgängig des Feldes), bei offenem Fenster, bei offener Rückfrage, im Fenster „Über“, bei offenen Rückfragen der Felder Projekte und Versionen, bei gedrückter Maustaste und während ein Export schreibt. Gesperrt heißt: die Taste bleibt unverbraucht, es gibt keine Meldung. Menü und Knöpfe gehen auch bei offenem Fenster und schließen es; ungespeicherte Eingaben darin gehen verloren. In alten Versionen (nur lesbar), bei offener Rückfrage und während ein Export schreibt, sind sie gesperrt, mit Grund im Hinweis.
- **Hinweis in der Statuszeile:** `AppState::info` (neu, getrennt von `message`) zeigt „Rückgängig: <Aktion>“ bzw. „Wiederholen: <Aktion>“ in der normalen Textfarbe (nicht in der Fehlerfarbe). Er bleibt stehen, bis ein neuer Schritt entsteht oder eine Meldung kommt; in der Skizze steht er hinter „Skizzen-Modus: <Name> ·“. Eine Meldung hat Vorrang.
- **Namen** der Schritte (`planora-app/src/undo_labels.rs`) werden aus dem Unterschied von vorher und nachher abgeleitet, nicht an den Aufrufstellen gesetzt; die Reihenfolge der Regeln steht im Quelltext. Ohne passende Regel heißt der Schritt „Ändern“. Netz-Körper: „Netz importieren“, bei Lageänderung „Verschieben“.
- **Dateien:** neu `history.rs`, `undo_labels.rs` (beide `planora-app`); `Document::restore`, `Snapshot::same_content/same_model`, `RestoreKind`, `PartialEq` für Skizzen (`planora-feature`); `AppState::restore_document`; `DocTab` mit Verlauf. Kernel, Netz, STEP, Austausch, Projekt, Skizze, Zeichnung, Render, Mathe, Abhängigkeiten, `deny.toml` und das Dateiformat sind unverändert.

### Festlegungen R1 bis R10 und Abweichungen
Die Festlegungen stehen im Kopf des Plans und kurz in der Spec §10.
- **R1 Zähler steigen immer:** `Document::restore` setzt `edits` und `drawing_edits` nie zurück (voller Weg: beide alt + 1; Zeichnungsweg: `drawing_edits` + 1). Feature-Versionen und Netz-Kennungen steigen ebenfalls weiter (über den Auftrag hinaus: sonst träfe ein offener Dialog eine alte Version zufällig wieder, und eine Netz-Kennung würde nach Rückgängig eines Imports neu vergeben).
- **R2 Zwei Wege:** Gleichen sich Features, Marker, Nummern und Netze, werden nur Zeichnungen, Projektangaben und Stücklistentexte ersetzt (keine Neuberechnung, Ansichtsergebnisse bleiben). Sonst wird wie beim Laden neu gerechnet; bereits geprüfte Netze (derselbe `Arc`) nicht noch einmal. Nach dem vollen Weg sind alle Körper neue Objekte.
- **R3 Schritt = Bild vorher**, kein Schritt bei inhaltsgleichem Bild. Ein Dokument mit NaN-Parameter ist nie gleich mit sich selbst; der Zähler entscheidet zuerst, es entstehen keine Scheinschritte je Bild (Grenze: eine echte Zähleränderung ohne Inhaltsänderung gibt dort einen überflüssigen Schritt).
- **R4 Geste:** offen, solange eine Maustaste gedrückt ist oder ein Feld die Tastatur hat.
- **R5 Skizzen** gelten als gleich bei gleichem Stand (`sketch.revision()`), gleicher Ebene, gleichem Namen und gleichen Kopien; ungleicher Stand bei gleichem Inhalt gilt als geändert.
- **R6 Namen** aus dem Unterschied; **R7 Gespeichert-Marke** an Schrittnummern; **R8 Sperren** wie oben; **R9 Namenszähler** werden aus den Namen neu gebildet (nach Rückgängig von „Quader 3“ heißt der nächste wieder „Quader 3“); **R10 Skizze** wie oben.
- **Abweichung Knöpfe:** Der Plan sah die zwei Knöpfe im Befehlsfeld unten vor. Dort fehlten bei 1100 × 700 etwa 36 px (der Knopf „Fase …“ ragte heraus, der Test `every_button_is_reachable_at_narrow_widths` war rot). Auf Entscheidung des Nutzers stehen sie im **oberen Feld** neben den Menüs. Das untere Feld und seine Tests sind unverändert.
- **Abweichung Tab-Leiste (Option A):** Mit den Knöpfen bleiben der Tab-Leiste bei 1100 Pixel nur etwa 100 px. Das Symbol „Neu“ (+) steht deshalb fest hinter der rollenden Leiste und ist immer sichtbar und klickbar; ein Trennstrich hinter den Knöpfen entfällt. Bei 1100 Pixel rollt die Leiste schon ab einem Tab; Name und Schließkreuz des aktiven Tabs sind **ab 1150 Pixel** in allen Fällen ganz sichtbar (getestet mit 1 bis 9 Tabs, geändert und ungeändert). Bei 1140 Pixel gilt das für ungeänderte Tabs; bei einem geänderten aktiven Tab ragt das Schließkreuz bei 4, 7, 8 oder 9 Tabs 0,1 bis 0,6 px hinaus (Breite der Tabnummer). Bei 1130 Pixel gilt es für ungeänderte Tabs (gemessen mit 1, 2 und 5 Tabs). Darunter schließt Strg+W den aktiven Tab (getestet). Bei 800 × 600 steht das Feld wie bisher über den Spalten, dort passt alles.
- **Abweichung `drawing_dialog.rs`:** Scheitert die erste Ansicht einer neuen Zeichnung, stellt `NewDrawingDialog::create` den Stand davor mit `Document::restore(before)` ganz wieder her (statt nur die Zeichnung zu entfernen). Vorher blieb der Zeichnungszähler erhöht, das Bild war ungleich, und es entstand ein Scheinschritt ohne sichtbare Änderung.
- **Abweichung Skizze ohne `Tracker`:** Der Skizzenverlauf hat keinen `Tracker`; er vergleicht den Entwurf je Bild mit dem letzten Eintrag über den Skizzenstand (der Aufschub durch ein offenes Werkzeug verträgt keinen verbrauchenden Beobachter). Zusätzlich geändert gegenüber den Dateilisten der Aufgaben: `history.rs` (`sketch_bytes`), `sketch_panel.rs` (Maßfeld schließt nach einem Sprung), `undo_labels.rs` (Name ohne die offenen Punkte).
- **Zeichnungskennungen werden nach Rückgängig wiederverwendet:** Nimmt Rückgängig eine Zeichnung zurück und wird danach eine neue angelegt, bekommt sie dieselbe Kennung. Die Oberfläche vergisst Auswahl, Zoom und Verschiebung der fehlenden Zeichnung (`forget_missing`), die neue beginnt frisch.
- **Menü Bearbeiten** ist in alten Versionen als Ganzes ausgegraut (bestehende Logik); der Hinweis des Eintrags ist dort nicht erreichbar.
- **Strg+Alt+Z** (und Strg+Alt+Y) lösen ebenfalls aus. Das galt, bis die Tastenkürzel genaue Umschalttasten bekamen (Teilprojekt Tastenkürzel, unten): Strg+Alt+Z löst jetzt nichts mehr aus.
- Strg+Z in dem Bild, in dem ein Fenster schließt, bleibt ohne Wirkung; bei offenem Menü wirkt Strg+Z. Jedes fokussierte Widget (auch ein Knopf nach Tab) sperrt über „Tippen“. Mehrere Tastenereignisse in einem Bild zählen als ein Schritt.

### Grenzen
- Der Verlauf liegt nur im Speicher: nicht in der Datei, nicht über den Programmstart, nicht zwischen Tabs. Das Dateiformat ist unverändert.
- **Namen aus dem Unterschied, mitunter allgemein:** Fallen mehrere Änderungen in ein Bild, gilt die feste Reihenfolge der Regeln, selten heißt der Schritt „Ändern“. Bekannte Eigenheiten: Ist in einem Bild ein Feature neu und ein anderes gelöscht, heißt der Schritt nach dem neuen; bei mehreren geänderten Features gilt das erste; ein geänderter Maßwert in der Skizze heißt „Skizze ändern“ (es gibt keine Regel „Maß ändern“, auch nicht bei offenem Klick); das Spiegeln einer einzelnen Kurve heißt nach der Kurve („Kreis zeichnen“), das mehrerer „Zeichnen“; Maße für Winkel oder Ellipsenachsen zählen als „Maß setzen“.
- **Nach einem fehlgeschlagenen Zurückladen** fällt dieser Schritt aus dem Verlauf, der nächste Schritt fasst dann zwei Änderungen zusammen. Das Dokument bleibt dabei unverändert, es kommt die Meldung „Rückgängig ist nicht möglich.“ (bzw. „Wiederholen ist nicht möglich.“). Das Programm erzeugt so einen Fehler nicht von selbst; er ist nur über einen unbrauchbaren Eintrag im Test erreichbar.
- **Skizzen mit gleichem Inhalt, aber neuem Stand** gelten als geändert (höchstens ein überflüssiger Schritt, etwa „Skizze ändern“ ohne sichtbaren Unterschied nach einem Lösen, das Werte nur bitweise verschiebt).
- Flächen- und Kantenwahl sind nach einem vollen Zurückladen leer; der gewählte Körper wird nur über Verschieben, Verrunden und Boolesche Operationen mitgeführt. Ein zurückgenommener Quader verliert die Wahl.
- Die Zeichnungsansichten rechnen nach einem Schritt im Modell neu (nicht gemessen). Das Zurückladen läuft auf der Oberfläche; ein Fortschrittsbalken ist nicht nötig (siehe Zahlen).
- Das Zahlenfeld „Maß“ im Skizzenfeld wird nach einem Sprung geschlossen. Tippen sperrt Strg+Z ohnehin.
- **Speichergrenze geschätzt:** Die 200 MB zählen Netze nicht mit (geteilte `Arc`). Netze, die nur noch Bilder im Verlauf festhalten (etwa nach mehrmaligem Importieren und Löschen großer Netz-Körper), werden nicht gezählt; der tatsächliche Speicher kann die Grenze dann deutlich übersteigen. Später denkbar: Netze zählen, die nicht mehr im Dokument sind.
- **Hinweis je Tab:** Der Hinweis „Rückgängig: …“ (`AppState::info`) gehört zum Tab. Nach einem Tabwechsel und zurück steht ein alter Hinweis noch da, bis in diesem Tab der nächste Schritt entsteht (Regel „bis zum nächsten Schritt“). Landet Rückgängig oder Wiederholen auf einem Stand mit fehlerhaftem Feature, entfällt der Hinweis, und die Statuszeile zeigt den Fehler in der Fehlerfarbe (außer in der offenen Skizze, dort steht die Skizzenzeile vor Feature-Fehlern).
- **Nur 18,75 px Luft bei 1100 Pixel:** Jede weitere Verbreiterung von Menü oder Knöpfen schiebt das obere Feld über die Seitenspalten (die Tests fangen das).
- **Offene Ideen für das obere Feld** (nicht umgesetzt): **B** Knöpfe aus der Zeile nehmen (zum Beispiel ins Statusfeld oder an den linken Rand der freien Fläche): gibt der Tab-Leiste 60 px zurück, dann passen Name und × auch bei 1100 für „Unbenannt n ●“. **C** Das Feld bei 1100 über die Seitenspalten rücken lassen (Mindestbreite = Menü + Knöpfe + aktiver Tab + × + „Neu“): verlangt, die Tests `every_button_is_reachable_at_narrow_widths` und `status_stays_readable_with_the_real_bottom_field` zu ändern, nur mit Freigabe. **D** Menüs verkürzen (zum Beispiel einzelne Menüs als Symbol).
- Die Knöpfe sind 20 × 24 px (schmaler als die 24-px-Symbolknöpfe unten); die Optik ist noch nicht von Hand geprüft.
- Nicht von Hand im echten Fenster geprüft (offen): alles unter „Handliste“; ausdrücklich, dass der Hinweis in der Statuszeile nach Rückgängig sichtbar bleibt (nicht nur ein Bild lang).

### Zahlen
- **Zurückladen (Aufgabe 2, Ryzen 9 9950X, Median aus 5 Läufen):** schweres Modell (Platte mit 10 × 10 Bohrungen, Spline-Extrusion, Netz-Körper mit 200 000 Dreiecken, 6 Ansichten mit 2 Schnitten, 30 Maße) im Release-Build **etwa 289 ms** (`from_snapshot` 250 ms + Auffrischen 39 ms; je nach Lauf 230 bis 290 ms); mittleres Modell (37 Features, 4 Ansichten) 235 ms. Das Bild machen (`snapshot()`) kostet 0,0 ms, die Schätzung der Größe 0,00 ms. Im Debug-Build 962 ms (mittel) und 1209 ms (schwer), das betrifft nur Tests. Grenze des Plans war 1 s bzw. 50 ms: eingehalten, das Zurückladen bleibt synchron. Die Zeit entsteht fast ganz in der Neuberechnung der Features; die Netzprüfung ist nebensächlich.
- **Mutationsproben:** 249 Einträge (einzelne Mutationen) aus den Berichten der Aufgaben 1 bis 10 (`.superpowers/sdd/2026-10-19-rueckgaengig/`, Summenliste in `task-11-report.md`), jede rot außer fünf begründet gleichwertigen (grün, bleiben als Schutz). Dazu 244 Einzelproben in den Reviews.
- **Platz bei 1100 × 700:** oberes Feld Mindestbreite 525,25 px (ohne Knöpfe 487,25), Platz zwischen den Spalten 544 px, also 18,75 px Luft. Feld 544 = Rand 26 + Menü 319,25 + Knöpfe mit Abständen 60 + Tab-Leiste 100,75 + „Neu“ mit Abstand 38.
- **Gates (Stand der Abschlussprüfung):** `cargo fmt --all --check` und `cargo clippy --workspace --all-targets -- -D warnings` sauber; `cargo test --workspace -j 2 --no-fail-fast` (Debug): 303 Testläufe, 2932 Tests bestanden, 0 fehlgeschlagen, 40 ausgeschaltet; `cargo deny check`: advisories, bans, licenses und sources ok (nur Hinweise auf doppelte Versionen). Keine neue Abhängigkeit. Geschützte Pfade (`planora-kernel`, `planora-mesh/src`, `planora-step`, `planora-exchange`) bitgleich zu `6685f3e`; `Cargo.toml`, `Cargo.lock`, `deny.toml` ohne Unterschied.

### Handliste für den Nutzer (Prüfung von Hand im echten Fenster, Stand 2026-10-20)
Release-Build, Fenster zuerst 1400 × 900. Jeweils prüfen: Der Hinweis unten nennt die Aktion, der Punkt am Tab stimmt, nichts ragt heraus. Abhaken:

**A. Körper und Tasten**
- [ ] 1. Einen Quader anlegen. Strg+Z: Der Quader verschwindet, unten steht „Rückgängig: Quader anlegen“ in normaler Schrift (nicht rot). Strg+Y: er kommt zurück („Wiederholen: Quader anlegen“). Strg+Umschalt+Z tut dasselbe wie Strg+Y. Die Kamera bleibt, wo sie war.
- [ ] 2. Noch einmal Strg+Z, dann das **Menü Bearbeiten** öffnen: oben „Wiederholen: Quader anlegen“ ist aktiv, „Rückgängig“ ist grau mit dem Hinweis „Es gibt nichts rückgängig zu machen.“ Den Eintrag „Wiederholen“ anklicken. Dann dasselbe mit den **zwei Knöpfen** im oberen Feld neben den Menüs (Hinweis mit Aktionsname und Kürzel).
- [ ] 3. Quader wählen, im Feld „Position“ X mit der Maus ziehen (oder eine Zahl tippen und Enter): Ein einziges Strg+Z stellt die alte Lage her (nicht in Stücken), Strg+Y die neue. Der Körper bleibt gewählt, das Feld bleibt sichtbar.
- [ ] 4. Zwei Körper und eine Skizze oder Rundung darauf, dann den ersten Körper mit „Löschen mit Folgen“ löschen: ein Strg+Z bringt alles zurück (gleiche Namen, Körper sichtbar, Zeitleistenmarke wie vorher); Strg+Y löscht wieder alles auf einmal.
- [ ] 5. Zwei Körper vereinigen: Strg+Z bringt beide Ausgangskörper zurück, Strg+Y das Ergebnis. Eine Kante verrunden: Strg+Z nimmt die Rundung zurück. Ein Feature umbenennen und eins unterdrücken/einblenden: je ein Schritt mit richtigem Namen.
- [ ] 6. Nach Rückgängig einen neuen Quader anlegen: Er heißt wieder wie der zurückgenommene („Quader 3“, nicht „Quader 4“), „Wiederholen“ ist danach grau.

**B. Skizze**
- [ ] 7. Skizze auf einer Ebene öffnen, mit dem Linienwerkzeug einen Linienzug aus drei Linien zeichnen. Strg+Z: die letzte Linie verschwindet, das Dokument und der Baum bleiben. Bei offenem ersten Klick (nur ein Punkt gesetzt): das erste Strg+Z nimmt nur den Punkt zurück, das zweite die Linie davor.
- [ ] 8. Einen Skizzenpunkt mit der Maus ziehen: ein Strg+Z stellt ihn wieder her. Ein Maß setzen und den Wert ändern: Strg+Z (der Schritt heißt „Skizze ändern“, das ist so gewollt).
- [ ] 9. „Skizze beenden“: Strg+Z nimmt die ganze Skizze auf einmal zurück (nicht Linie für Linie). Die Zeile „Skizzen-Modus: …“ bleibt unten sichtbar, auch nach Rückgängig.

**C. Zeichnung**
- [ ] 10. Zeichnung anlegen (mit Vorderansicht): Strg+Z nimmt die Zeichnung zurück (der Bereich wechselt auf das Modell), Strg+Y holt sie zurück, das Blatt zeigt sofort den richtigen Stand.
- [ ] 11. Eine Ansicht verschieben (ziehen), ein Maß anlegen, eine Positionsnummer setzen, einen Schnitt anlegen, Blattangaben ändern: jeweils Strg+Z und Strg+Y. Das Blatt stimmt sofort, keine alte Ansicht, kein Flackern. Das Ziehen einer Ansicht ist ein Schritt.

**D. Speichern und Punkt**
- [ ] 12. Speichern: kein Punkt am Tab. Eine Änderung: Punkt. Strg+Z: kein Punkt (gleicher Stand wie gespeichert). Strg+Y: Punkt. Zweimal Strg+Z (hinter den gespeicherten Stand): Punkt. Rückgängig geht nach dem Speichern noch.
- [ ] 13. Eine Änderung machen, die Autosicherung abwarten, dann zurück bis zum gespeicherten Stand und den Tab schließen: keine Rückfrage, und beim nächsten Öffnen wird keine Wiederherstellung angeboten.
- [ ] 14. Zwei Tabs mit verschiedenen Dokumenten: Strg+Z wirkt nur im aktiven Tab, jeder Tab hat seinen eigenen Verlauf (das Menü zeigt andere Namen).

**E. Sperren und Fenster**
- [ ] 15. Ein Zahlenfeld (zum Beispiel „Position“) anklicken, etwas tippen und Strg+Z: Das Feld nimmt seine Eingabe zurück, das Dokument bleibt unverändert. Nach Escape oder einem Klick daneben wirkt Strg+Z auf das Dokument.
- [ ] 16. Das Fenster „Quader erzeugen“ (oder „Ändern“, „Umbenennen“) offen lassen und Strg+Z drücken: nichts geschieht, das Fenster bleibt. Dann im Menü Bearbeiten „Rückgängig“ wählen: Das Fenster schließt, die Eingabe darin ist weg. Dasselbe mit dem Knopf. Bei der Rückfrage „Löschen mit Folgen“ sind Menü und Knöpfe gesperrt (Hinweis mit Grund).
- [ ] 17. Mit gedrückter Maustaste (Position ziehen) Strg+Z drücken: wirkt nicht; nach dem Loslassen geht es.
- [ ] 18. Eine alte Version aus der Versionsliste öffnen (nur lesbar): Menü und Knöpfe sind grau mit Hinweis, die Tasten tun nichts. Beim Export in ein Fenster, solange die Datei geschrieben wird, ist Rückgängig gesperrt.

**F. Aussehen und Fenstergrößen**
- [ ] 19. Hell und Dunkel: Knopfsymbole gut lesbar, gesperrt gedämpft, beim Darüberfahren heller; der Hinweis unten in normaler Textfarbe, nicht rot.
- [ ] 20. Fenster 1100 × 700: Die zwei Knöpfe sitzen zwischen Menü und Tab-Leiste, das obere Feld steht zwischen den Seitenspalten und überlappt nichts; das „+“ ist immer sichtbar, auch mit vielen Tabs; die Tab-Leiste rollt, Strg+W schließt den aktiven Tab.
- [ ] 21. Fenster 800 × 600: Knöpfe, Menü, Tab-Leiste und „+“ sichtbar, nichts abgeschnitten (das Feld steht dort über den Spalten wie bisher).
- [ ] 22. Fenster ab 1150 Pixel Breite: Name und Schließkreuz des aktiven Tabs ganz zu sehen, auch mit Punkt.
- [ ] 23. Ein sehr großes Modell (Netz-Körper) und Strg+Z: Das Zurückladen dauert deutlich unter einer Sekunde. Falls nicht, bitte melden (dann kommt eine Fortschrittsanzeige).
- [ ] 24. Der Hinweis „Rückgängig: …“ unten bleibt stehen, bis der nächste Schritt kommt (nicht nur einen Augenblick), auch nach Mausbewegungen.

## Teilprojekt: Parameter und Gleichungen

### Stand und Geliefertes
Stand: 2026-10-21 (Zweig `feature/parameter`, Basis `master` @ `ad1da89`, Spec @ `7f571c6`). Ein Dokument hat eine Parametertabelle; die Zahlenfelder der Körper-Funktionen und die bestimmenden Skizzenmaße nehmen Formeln an; ändert man einen Parameter, rechnet das Modell als **ein** rücknehmbarer Schritt neu.

- **Formelbaustein** (`planora-feature/src/expr.rs`, rein, nur `std` und `thiserror`): Zerleger, Auswertung mit Einheiten (Länge, Winkel, Zahl), Kreiserkennung, Umbenennen Wort für Wort. Operatoren `+ - * / ^`, Klammern, `sqrt sin cos tan abs min max round`, `pi`; Winkel in Grad.
- **Parametertabelle im Dokument** (`params.rs`): anlegen, ändern, umbenennen, löschen, „Durch den Wert ersetzen“; jede Änderung ist eine Operation mit Ablehnung ohne Wirkung (Dokument und Zähler bleiben gleich). Teil des Bildes (`Snapshot`), damit Speichern, Laden und Rückgängig sie tragen.
- **Formeln in Körper-Features** (`formulas.rs`): Nebentabelle je Feature (Feldschlüssel → Text), ein Durchgang vor jeder Neuberechnung schreibt die Werte in die `f64`-Felder. Felder: Quader 6, Zylinder und Kegel je 5, Kugel 4, Extrusion: Höhe, Verschieben Δx/Δy/Δz, Rundung: Radius, Fase gleich 1, Fase zwei 2, Fase mit Winkel 2, Netz-Körper: Position x/y/z und Maßstab. Skizzenmaße über die Kennung der Abhängigkeit, mit dem Rechenstatus des Lösers (Widerspruch oder keine Lösung ergibt den Fehlerstatus).
- **Dateiformat 9** (`planora-project`): nur bei Parametern oder Formeln; alle Golden-Dateien Byte für Byte unverändert. Strenges Lesen mit deutschen Meldungen.
- **App:** Menü „Bearbeiten → Parameter …“ (Symbol Phosphor `function`), Fenster „Parameter“ mit Tabelle, Formelfeld mit „fx“ und Wertanzeige, Formelfelder in allen Erzeugen-Fenstern, im Fenster „… bearbeiten“, im Rundungsfenster, im Maßfeld der Skizze, in der Maßliste (Zeile „fx“) und im Eigenschaftenfeld des Netz-Körpers. Schrittnamen „Parameter anlegen/ändern/löschen“, „Formel ändern“; Verlaufsgröße zählt Tabelle und Formeltexte.
- **Dateien:** neu `expr.rs`, `params.rs`, `formulas.rs` (`planora-feature`), `formula_field.rs`, `parameter_dialog.rs` (`planora-app`); geändert `ops.rs`, `snapshot.rs`, `model.rs`, `sketch_feature`-Teile, `convert.rs`/`format.rs` (`planora-project`), die Dialoge, `state.rs`, `app.rs`, `history.rs`, `undo_labels.rs`. Kernel, Netz, STEP, Austausch, Skizze, Zeichnung, Render, Mathe, `Cargo.toml`, `Cargo.lock`, `deny.toml` und alle Golden-Dateien sind unverändert (geprüft gegen `7f571c6`).

### Festlegungen P1 bis P14 mit Abweichungen
Die Festlegungen stehen im Kopf des Plans und kurz in der Spec §8. Einzelheiten und alle Abweichungen: Plan, Abschnitt „Abweichungen bei der Umsetzung“.
- **P1 Werte schreiben:** Vor jeder Neuberechnung (Änderung, Laden, Zurückladen) wertet `Document` erst die Tabelle, dann alle Formeln aus und schreibt die Ergebnisse in die `f64`-Felder. Nur Features, deren Werte oder Formelfehler sich ändern, bekommen eine neue Version. *Abweichung:* Bei einer Skizze mit geänderten Formeln (Umbenennen, Ersetzen) steigt die Version immer, damit kein gespeicherter Fehlschlag die Änderung überdauert.
- **P2 Zahl statt Formel:** Eine Operation ohne Formelangabe, die den Wert eines Feldes mit Formel ändert, löscht nur diese Formel. Formeln für Felder, die es nach einem Artwechsel nicht mehr gibt, entfallen. Gilt auch für Skizzen (getippte Zahl gilt).
- **P3 Strenge Prüfung, milder Ladeweg:** Ein „Formelfehler“ (Formel selbst kaputt) wird beim Ändern abgelehnt, ein „Wertfehler“ (Ergebnis außerhalb des Bereichs) nicht. *Abweichung:* Ein Wertfehler schreibt den Wert trotzdem ins Feld (P3 sagte „letzter Wert“); der Status ist Fehler. Nur für Formelfehler bleibt der letzte Wert. Ein alter Formelfehler gilt als schon vorhanden, wenn Feature, Feld und Formeltext gleich sind (beim Umbenennen mit dem umgeschriebenen Text). Skizzenmaße behalten bei Wertfehlern ihren letzten gültigen Wert, weil die Skizze den Wert selbst prüft.
- **P4 Einheiten:** Ausdrücke rechnen mit Exponenten von Länge und Winkel. Ein Feld nimmt seine Einheit oder eine reine Zahl; eine Länge passt nicht in den Maßstab. Die Extrusionshöhe muss größer als die Toleranz sein; Δ und Netz-Lage haben kein Vorzeichen-Gebot; der Maßstab liegt zwischen den Grenzen des Eintippens. Die Meldungen nennen Werte verlustfrei, wo eine gerundete Zahl dem Satz widerspräche (Maßstab, Winkel 0,999 und 179,004).
- **P5 Komma:** Zwischen zwei Ziffern ist das Komma Dezimalzeichen; sonst trennt es Werte („; “ oder „, “). `1,5,3` ist ein unerwartetes Zeichen, nie ein stiller Trenner.
- **P6 Namen:** ohne Rücksicht auf Groß- und Kleinschreibung; Umbenennen ersetzt ganze Namen (Zerleger, nicht Textsuche); Namen bis 64, Kommentare bis 200 Zeichen. *Abweichung:* Das Projekt hat daneben eine eigene Grenze von 500 Zeichen für Projektkommentare (zwei Konstanten `MAX_COMMENT_CHARS`, getrennte Namensräume).
- **P7 Durch den Wert ersetzen:** Jede Formel, die den Parameter direkt nennt, wird durch ihren Wert ersetzt, dann wird der Parameter gelöscht; ein Schritt, keine Wertänderung. *Abweichung (Endregel):* Abgelehnt wird bei Formel- oder Wertfehler eines Benutzers, bei Skizzen im Fehler oder übersprungen und bei nicht berechneten Skizzen (hinter dem Marker, unterdrückt), deren letzte Formel entfiele und die sich nicht lösen lassen. Körperfelder ohne Formel- oder Wertfehler tragen schon den Formelwert und dürfen ersetzt werden, auch wenn das Feature aus anderem Grund im Fehler ist. Zu lange Werte (über 200 Zeichen) werden mit Meldung abgelehnt.
- **P8 Strg+Z mit dem Fenster „Parameter“:** Das Fenster zählt nicht zu den sperrenden Fenstern; Strg+Z wirkt, wenn kein Feld die Tastatur hat; danach lädt es seine Zeilen neu und verwirft Entwürfe.
- **P9 Übernahme:** Fenster mit Knopf übernehmen erst mit dem Knopf; die Anzeige „= Wert“ rechnet beim Tippen mit. Zeilen des Fensters „Parameter“ und Netz-Formelfelder wirken bei Enter oder Verlassen. *Abweichungen:* Das Fenster läuft im Bild vor allen übrigen Bereichen; eine noch offene andere Änderung wird zuerst ein eigener Schritt (so wird ein Ziehen nicht mit „Parameter ändern“ verschmolzen); nach der Übernahme entsteht der Schritt sofort. Verlassen übernimmt, wenn der Entwurf vom Dokument abweicht; ein aus dem Bild gerolltes Feld mit Entwurf wird übernommen wie beim Verlassen.
- **P10 Netz-Körper:** Position x/y/z und Maßstab sind Formelfelder (Tippen statt Ziehen); die Drehung bleibt ein Ziehfeld. Der 3D-Griff zieht weiter, entfernt aber die Formel der gezogenen Achse.
- **P11 Verschieben mit Formeln:** Ein Verschieben mit Formeln wird beim Ziehen oder im Positionsfeld nie erweitert; es entsteht ein neues Verschieben.
- **P12 Skizzenstatus:** Eine Skizze mit mindestens einer Formel bekommt den Fehlerstatus, wenn der Löser nicht „gelöst“ meldet, auch wenn sie schon vorher ungelöst war. *Abweichung (Maßfeld):* Eine Eingabe, die die Skizze neu widersprüchlich macht, wird wie bisher abgelehnt; war sie schon vorher ungelöst und hat Formeln, steht im Maßfeld rot derselbe Text wie im Fehlerstatus, und die Ansicht zeigt die gespeicherte Lage.
- **P13 Neue Zeile:** „ParameterN“ (kleinstes freies N), Formel „10“, Länge, ohne Kommentar.
- **P14 Meldungsgrammatik:** „Den Namen „Breit“ gibt es nicht.“

Weitere Abweichungen im Überblick (Einzelheiten im Plan): Kreis- und Selbstbezugsmeldungen nennen die Namen ohne Vorsilbe; Umbenennen auf einen vorhandenen Namen meldet den getippten Namen; ein Bild mit Formel an einem Feld, das die Art nicht hat, wird abgelehnt; Dateien mit Formeln an gelöschten, anzeigenden oder nicht bestimmenden Skizzenabhängigkeiten werden abgelehnt (das stille Aufräumen bleibt nur für Rückgängig-Bilder), Formeln an abgeschalteten Maßen laden und folgen weiter dem Parameter; Fremdtexte aus Dateien stehen in Meldungen auf höchstens 40 Zeichen gekürzt; alle Längengrenzen zählen Zeichen, nicht Bytes; alle Erzeugen-Fenster bleiben bei einem Fehler offen; ein offenes Fenster „… bearbeiten“ schließt nach jeder Parameteraktion; Netz-Körper-Position als drei Zeilen; im Bearbeiten-Fenster meldet ein Feld der variablen Rundung mit Text „Hier sind nur Zahlen möglich.“; „fx“ heißt für Bildschirmleser „In Wert umwandeln“.

### Grenzen
- **Profilnummer bei Extrusion:** Eine Extrusion wählt ihr Profil über die Nummer. Ändert eine Parameteränderung die Reihenfolge der Profile, kann ein anderes Profil gewählt sein.
- **Hinweispunkte bei großen Sprüngen:** Flächen-Hinweispunkte können bei sehr großen Sprüngen verloren gehen.
- **Löser startet von der gespeicherten Lösung:** Große Sprünge können eine andere Lösung wählen oder nicht konvergieren (dann Fehlerstatus statt stiller falscher Form).
- **Netz-Körper:** Position und Maßstab per Tippen; der 3D-Griff entfernt die Formel der gezogenen Achse. Die Drehung hat keine Formel.
- **Rechengenauigkeit:** Bei Winkeln, die keine runden Vielfachen sind (30°, 45°, 60°, 90° rechnen genau), und bei Potenzen rechnen sin, cos, tan und „^“ mit der Mathematikbibliothek des Betriebssystems; die letzte Stelle des Ergebnisses kann deshalb zwischen Betriebssystemen abweichen (auf demselben Rechner kommt immer genau dasselbe heraus).
- **Zeichnungsmaße ohne Formeln;** anzeigende Skizzenmaße ohne Formeln; variable Rundung und das Positionsfeld des Körpers ohne Formeln.
- **Keine Auswahlliste** der Namen im Feld; nur der Hinweis „Namen: …“ (höchstens eine feste Zahl Namen, danach „…“).
- **Fase 179°:** als Zahl zulässig, scheitert an einer rechtwinkligen Kante im Kernel mit dessen Meldung.
- **Zahlen:** höchstens 500 Parameter, Namen 64, Kommentare 200, Formeln 200 Zeichen, Tiefe 64.
- **Fenster „Parameter“:** Bis 60 Zeilen werden alle gezeichnet, darüber nur die sichtbaren plus je eine davor und dahinter. Ein fokussiertes Feld, das aus dem Bild rollt, wird übernommen wie beim Verlassen.
- **Schrittname:** „Formel ändern“ entsteht, wann immer sich Formeln ändern, auch wenn sich der Wert mitändert.
- **Winkel im Maßfeld:** Das Feld zeigt Grad mit einer Nachkommastelle wie bisher; gespeichert wird `wert.to_radians()`, bitgleich mit dem Durchgang des Dokuments. Eine Skizze, die schon ungelöst ist, nimmt in der offenen Skizze weiter Formeln an; P12 greift beim Beenden und als rote Meldung im Feld.
- **Lücken in den Tests (bewusst angenommen):** Sofortiges Entfernen der Formel beim Haken „anzeigend“ ist doppelt abgesichert (eine Mutation dort bleibt grün); gemeinsame Textkonstanten des Lösers; einzelne Randfälle der Meldungen sind in den Reviews vermerkt.
- **Nicht von Hand im echten Fenster geprüft** (offen): alles unter „Handliste“.

### Zahlen
- **Bildzeiten des Fensters „Parameter“ (Release-Build, Ryzen 9 9950X, ohne Tessellieren):** 60 Zeilen (alle gezeichnet) 0,74 ms je Bild; 500 Zeilen (nur sichtbare) 0,20 ms. Vor dem Rollen waren es 6,09 ms für 500 Zeilen.
- **Mutationsproben:** 294 Einträge (einzelne Mutationen) aus den Berichten der Aufgaben 1 bis 11 (`.superpowers/sdd/2026-10-20-parameter/`, Summenliste in `task-12-report.md`), jede rot; die wenigen, die zunächst grün blieben, haben die Tests schärfer gemacht (Liste im Bericht). Dazu die Einzelproben der Reviews (zwischen 26 und 42 je Aufgabe).
- **Gates (Stand der Abschlussprüfung):** siehe `task-12-report.md`; `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 2 --no-fail-fast` und `cargo deny check` sauber. Keine neue Abhängigkeit.

### Handliste für den Nutzer (Prüfung von Hand im echten Fenster, Stand 2026-10-21)
Release-Build, Fenster zuerst 1400 × 900. Jeweils prüfen: Texte lesbar, nichts ragt heraus, Meldung nennt den Grund. Abhaken:

**A. Fenster „Parameter“**
- [ ] 1. Menü „Bearbeiten → Parameter …“ öffnen. Es erscheint das Fenster „Parameter“ mit den Spalten Name, Formel, Wert, Art, Kommentar. Mit „Zeile hinzufügen“ entsteht „Parameter1“, Formel „10“, Wert „10,00 mm“, Art „Länge“.
- [ ] 2. Name auf „Breite“ ändern, Formel auf „40“, Enter: Wert „40,00 mm“. Eine zweite Zeile „Tiefe“ mit Formel „Breite / 2“: Wert „20,00 mm“. Unten im Menü „Bearbeiten“ steht „Rückgängig: Parameter ändern“ bzw. „anlegen“.
- [ ] 3. Breite auf „60“ ändern: Tiefe zeigt „30,00 mm“. Strg+Z (Tastatur nicht im Feld): Breite wieder 40, das Fenster bleibt offen und zeigt den alten Stand; Strg+Umschalt+Z holt die 60 zurück.
- [ ] 4. Eine Zeile umbenennen („Breite“ zu „B“): Die Formel von „Tiefe“ lautet jetzt „B / 2“. Rückgängig bringt beides zurück.
- [ ] 5. Kommentar eintippen, Art auf „Winkel“ stellen: „Wert“ zeigt „°“. Zurück auf „Länge“. Namen mit Umlaut („Höhe“) und mit Leerzeichen („Zu breit“) tippen: Umlaut geht, Leerzeichen wird mit Meldung abgelehnt.

**B. Ungültige Eingaben**
- [ ] 6. Formel „Breit“ eintippen: rote Meldung „Den Namen „Breit“ gibt es nicht.“, Dokument unverändert. Formel „Breite + w“ mit einem Winkelparameter „w“: Meldung zu Länge und Winkel. Formel „1/0“: Meldung. `min(1,5)`: Meldung „min braucht mindestens zwei Werte“. Name „sin“: abgelehnt.
- [ ] 7. Kreis: „a = b + 1“ und danach „b = a“ eintippen: Meldung nennt „a“ und „b“, nichts ändert sich.
- [ ] 8. Eine Formel mit 201 Zeichen wird abgelehnt. Bei voller Tabelle (500 Zeilen) gilt: Meldung „Höchstens 500 Parameter.“ (die rote Meldung steht dann unter dem Knopf).

**C. Körper mit Formeln**
- [ ] 9. Quader erzeugen: Breite „Breite“, Tiefe „Tiefe“, Höhe „Breite / 4“. Die Anzeige „= 10,00 mm“ rechnet beim Tippen mit; „Erzeugen“ ist gesperrt, solange eine Formel ungültig ist. Nach „Erzeugen“ steht der Quader mit 40 × 20 × 10.
- [ ] 10. Zylinder (Radius `Breite / 4`), Kugel (Radius `Tiefe / 2`), Kegel (Höhe `sqrt(Breite^2 + Tiefe^2)`) erzeugen: Maße stimmen.
- [ ] 11. Breite auf 80 ändern: Alle Körper wachsen mit; ein einziges Strg+Z stellt alle vier wieder her.
- [ ] 12. Ändern-Dialog eines Quaders mit drei Formeln: „fx“ und „= …“ stehen bei den Feldern, nichts wird abgeschnitten. In einem Feld eine Zahl tippen („7“): nur diese Formel entfällt, die anderen bleiben. „fx“ anklicken: die Formel wird zur Zahl.
- [ ] 13. Verschieben: Δx `Breite / 2`, Δy `0`, Δz `Tiefe`. Der Körper wandert; Breite ändern, er folgt. Dann das Feld „Position“ des Körpers ziehen: Es entsteht ein neues Verschieben, die Formeln des ersten bleiben.
- [ ] 14. Extrusion: Skizze (Rechteck) beenden, extrudieren mit Höhe `Breite / 4`. Parameter ändern: Höhe folgt.
- [ ] 15. Rundung: Radius `Wand` an einer Kante. Fase mit Winkel: Abstand `Wand`, Winkel `w` (Winkelparameter). Parameter ändern, Körper folgt. Wand sehr groß: Eintrag im Baum zeigt den Fehler, Rückgängig stellt zurück.
- [ ] 16. Netz-Körper (eine Beispieldatei einlesen): im Eigenschaftenfeld stehen Zeilen „Position x“, „Position y“, „Position z“, „Maßstab“ mit Formelfeldern; Formel `Breite / 10` bei „Maßstab“ eintippen, Enter: Körper skaliert. Parameter ändern: die Zeile zeigt die Formel weiter, das Modell folgt. Die Drehung lässt sich weiter ziehen. 3D-Griff entlang einer Achse mit Formel ziehen: die Formel dieser Achse entfällt.

**D. Skizze**
- [ ] 17. Skizze: Rechteck mit Maß „Breite“ (Maßfeld zeigt „fx“, daneben „= 40,00 mm“). In der Maßliste steht unter dem Maß eine Zeile „fx Breite“. „OK“ ist bei ungültiger Formel gesperrt, die Meldung steht über „OK“.
- [ ] 18. Winkelmaß mit Formel `w` (in Grad) und Radius mit Formel `Breite / 4`: Werte stimmen, der Zeichenbereich zeigt Zahlen, nicht Formeln.
- [ ] 19. Dreieck mit Seiten a, b, c als Parameter; c auf 100 ändern (c > a + b): Die Skizze zeigt den Fehler „widersprechen sich Abhängigkeiten“, nichts Falsches wird gezeichnet, Rückgängig stellt zurück. Rechteck 30 auf 400: die Skizze folgt.
- [ ] 20. Anzeigendes Maß: Haken „anzeigend“ setzen; eine Formel hineintippen gibt „Ein anzeigendes Maß bestimmt nichts und kann keine Formel haben.“
- [ ] 21. Skizze offen lassen und das Fenster „Parameter“ öffnen: Es ist ausgegraut mit „Beenden Sie zuerst die Skizze.“

**E. Löschen und Ersetzen**
- [ ] 22. „Breite“ löschen, obwohl benutzt: „Breite wird in Quader 1, Skizze 1, Parameter Tiefe benutzt.“ „Durch den Wert ersetzen“: alle Körper bleiben gleich, die Formeln sind zu Zahlen geworden, Rückgängig bringt Parameter und Formeln zurück (ein einziger Schritt).
- [ ] 23. „Durch den Wert ersetzen“ mit einem Eintrag im Fehler (zum Beispiel Skizze mit widersprüchlichen Formeln): wird abgelehnt mit „… hat gerade einen Fehler. Beheben Sie zuerst den Fehler.“.
- [ ] 24. Ein Fenster „… bearbeiten“ offen lassen und im Fenster „Parameter“ etwas ändern: das Bearbeiten-Fenster schließt, unten steht „Der Dialog wurde geschlossen, weil sich Parameter geändert haben.“ Das Erzeugen-Fenster dagegen bleibt offen.

**F. Rückgängig und Datei**
- [ ] 25. Mehrere Parameteränderungen hintereinander mit Strg+Z zurücknehmen und mit Strg+Y wieder holen: Namen der Schritte stimmen („Parameter ändern“, „anlegen“, „löschen“, „Formel ändern“), der Punkt „ungespeichert“ folgt dem Verlauf.
- [ ] 26. Speichern, Tab schließen, Datei öffnen: Parameter, Formeln und Körper sind gleich (Format 9). Ein Dokument ohne Parameter speichern: Dateigröße und Inhalt wie vorher.
- [ ] 27. Eine Datei mit einer von Hand zerstörten Formel (Textfeld in der Datei ändern, zum Beispiel einen Namen falsch schreiben): sie lädt, der Eintrag im Baum zeigt den Fehler, das Feld hat den gespeicherten Wert. Eine Datei mit doppelten Parameternamen: wird mit deutscher Meldung abgelehnt.

**G. Tasten, Fenstergrößen, Aussehen**
- [ ] 28. Tipp-Test mit echten Tasten: In ein Formelfeld („Breite / 4“) Zeichen für Zeichen tippen: das Feld behält die Tastatur beim Wechsel von Zahl zu Formel und zurück, nichts geht verloren. Im Fenster „Parameter“ mit Tab durch die Zellen laufen (Tab springt am „fx“ vorbei vom Namen in die Formel; Enter oder Leertaste danach ändern die Formel nicht); „Zeile hinzufügen“ setzt die Tastatur in den neuen Namen.
- [ ] 29. Strg+Z beim Tippen im Fenster „Parameter“ wirkt auf das Feld, nicht auf das Dokument; nach Escape oder Klick daneben wirkt es auf das Dokument.
- [ ] 30. Mit gedrückter Maustaste (Position ziehen) bei offenem Fenster „Parameter“ den Wert ändern: Ziehen und Parameteränderung sind zwei Schritte.
- [ ] 31. Viele Zeilen (über 60) anlegen: das Fenster rollt flott, Tab führt in die Nachbarzeile, ein Eintrag aus dem Bild gerollt wird übernommen.
- [ ] 32. Fenster 1100 × 700 und 800 × 600: Das Fenster „Parameter“, die Formelfelder und die Fußzeile sind im Bild, nichts wird abgeschnitten; die Knöpfe sind erreichbar.
- [ ] 33. Hell und Dunkel: „fx“, rote Meldungen, Wertanzeige und das Symbol im Menü sind lesbar; Farben stimmen mit dem übrigen Programm.
- [ ] 34. Zwei Tabs mit verschiedenen Dokumenten: Parameter und Formeln eines Tabs erscheinen nicht im anderen, und ein getippter Formeltext in einem offenen Erzeugen-Fenster erscheint nicht im anderen Tab.

## Teilprojekt: STEP-Import

### Stand und Geliefertes
Stand: 2026-10-23 (Zweig `feature/step-import`, Basis `master` @ `c16d988`, Spec `docs/superpowers/specs/2026-10-21-step-import-design.md`, Plan `docs/superpowers/plans/2026-10-21-step-import.md`). „Datei → STEP importieren …“ liest STEP-Dateien (AP203, AP214, AP242, Text) und legt jeden lesbaren Volumenkörper als gewöhnlichen exakten Körper „STEP-Körper“ an, an der Lage aus der Datei, als **ein** rücknehmbarer Schritt. Die Form steht als normalisierter STEP-Text in der Projektdatei (Format 10). Kernel, Mathe, Netz, Austausch, Zeichnung, Skizze und Darstellung sind unverändert (geprüft gegen `c16d988`).

- **Leser** (`planora-step/src/read/`, 13 Dateien, keine neue Abhängigkeit): `lex.rs`, `index.rs`, `value.rs` (Zeichenstrom, Index, Einträge, feindliche Dateien), `units.rs` (Einheiten, Genauigkeit), `geom.rs` (Punkte bis Flächen), `spline.rs` (Freiformkurven und -flächen, Dreh- und Strangflächen), `topo.rs` (Schleifen, Orientierung, Parameterbereiche, Sicherheitsnetz gegen verdrehte Seiten), `seam.rs` (Nähte, Pole, Hohlräume), `weld.rs` (Zusammenziehen und Paaren mit Rastersuche), `assembly.rs` (Körper, Namen, Lagen), `import.rs` (Pipeline und Normalisieren), `error.rs`, `mod.rs`.
- **Dokument** (`planora-feature`): Formtabelle (`StepShapeId → Arc<StepShape>`), Feature-Art `StepBody`, Marken „STEP-Fläche n“, Summengrenze `MAX_STEP_TEXT_SUM_BYTES`.
- **Datei** (`planora-project`): Dateiformat 10, nur wenn ein STEP-Körper im Dokument ist; alle bisherigen Golden-Dateien Byte für Byte unverändert; neue Probe `tests/step_fixtures/format10_step.json`.
- **App:** Menü „Datei → STEP importieren …“ (Symbol `file-arrow-down`), Fenster „STEP importieren“ (Datei, Liste „Körper“ über die ganze Breite, „Einheit der Datei“, Fußzeile mit „Importieren“, „Andere Datei wählen …“, „Schließen“), Eigenschaftenfeld mit Warnung bei grober Genauigkeit, Schrittname „STEP importieren“, Symbol, Verlaufsgröße.
- **Tests:** `planora-step/tests/{read_parse, read_geometry, read_splines, read_topology, read_seams, read_seams_cases, read_weld, read_assembly, read_import, read_hostile, roundtrip, gate}.rs` und 11 handgeschriebene Dateien unter `tests/data/` (alle selbst geschrieben, Lizenz GPL-3.0-only), dazu `planora-feature/tests/step_bodies.rs`, `planora-project/tests/step_format.rs`, `planora-app/tests/{step_state, step_import_ui}.rs`.
- **Nur zwei interne Abhängigkeiten dazu:** `planora-feature → planora-step` und die Entwicklungs-Abhängigkeit `planora-step → planora-drawing` (für das Tor). `Cargo.lock` ändert sich nur um diese zwei Zeilen, `deny.toml` ist unverändert.

### Festlegungen P1 bis P16 mit Abweichungen
Die Festlegungen stehen im Kopf des Plans und kurz in der Spec §8. Einzelheiten und alle Abweichungen: Plan, Abschnitt „Abweichungen bei der Umsetzung“.
- **P1 Lesen in zwei Stufen:** Index, dann Zerlegen beim Gebrauch. Ein fehlender Verweis trifft nur die Körper, die ihn brauchen. *Zusatz:* `StepFile::entity_unchecked` (öffentlich); Aufrufer schützen sich selbst gegen Verweise im Kreis.
- **P2 Was ein Körper ist:** Eine Darstellung ergibt einen Körper (mehrere Volumen darin sind Schalen); mehrfach benutzte Teile ergeben mehrere Körper.
- **P3 Lagen:** Richtung über die Beziehungen der Baugruppe, Lagen bis zur Wurzel multipliziert. *Entscheidungen:* Beschädigte Einträge, die mit keinem Körper zu tun haben, lassen die Datei weiter lesbar. Eine Beziehung oder Abbildung, deren Enden nicht lesbar sind, lehnt die **ganze** Datei ab (es wäre nicht zu erkennen, welcher Körper sich bewegt). Ein Körper, dessen Vorkommen unlesbar ist, aber dessen Beziehung lesbar bleibt, liegt am Ursprung mit „Richtung unklar“. Wird die Richtung nur geraten, wird die Lage angewandt und das gemeldet. Eine Beziehung ohne Transformation neben einer Abbildung desselben Teils wird nicht doppelt als Körper am Ursprung gezählt.
- **P4 Genauigkeit:** 1e-6 bis 1e-3 mm, fehlend oder ungültig: 1e-3 mm mit Warnung. *Zusatz:* Werte ≤ 0 oder nicht endlich werden übergangen; gespeichert wird die Genauigkeit der Datei in der Form (`StepShape::tolerance_mm`, Bereich 1e-6 … 1e-3 mm, sonst Meldung).
- **P5 Normalisieren:** Schreiben und Wieder-Lesen mit festem Zeitstempel. *Abweichung:* Bis zu vier Durchgänge, bis der Text sich nicht mehr ändert (die erste Lesung zieht z. B. Ecken auf eine Kegelspitze). „Ruhend“ ist für die Testkörper erreicht (Kegel getestet bei ±1000 und ±10 000 mm, 80 von 80), aber nicht bewiesen. Der Schreiber behält den Bezugsradius eines Kegels, wenn dieser im Bereich der Fläche liegt.
- **P6 Keine Lagefelder:** Verschieben geht über das gewöhnliche Verschieben. *Folge (offen):* Das Menü „Bearbeiten …“ ist für STEP-Körper sichtbar und tut nichts, wie bei Netz-Körpern.
- **P7 Marken:** „STEP-Fläche n“, Reihenfolge des normalisierten Textes. Nach dem Laden sind sie gleich.
- **P8 Dreh- und Strangflächen:** Analytisch, wo es geht, sonst Freiform. *Abweichung (Fehler im Plan):* Bei einer Drehfläche ist u der Winkel und v die Kurve (ISO-Reihenfolge), nicht umgekehrt; so steht es im Leser. Eine Drehfläche aus einer schrägen Geraden wird abgelehnt.
- **P9 Nähte:** Nur Zylinder, Kegel und Kugel ohne Naht. *Folge (Dennis hat am 2026-10-23 entschieden: eigenes Teilprojekt, siehe unten):* Torusbänder ohne Naht werden abgelehnt. Pole werden geometrisch erkannt (nicht nur in der Parameterebene).
- **P10 Namen:** Wie geplant. *Abweichung:* Erzeugte Namen „STEP-Körper n“ steigen über die vorhandenen Namen im Dokument und in der Datei (`AppState::step_body_names`), damit die Teileliste nichts zusammenwirft.
- **P11 Einheit umstellen:** Wie geplant, auch für gemischte Dateien (alle Kontexte auf einmal). Bei Dateien mit mehreren Einheiten gilt: eine gewählte Einheit gilt für alle, die eigene Einheit der Datei hebt die Wahl wieder auf.
- **P12 Grenzen:** Wie geplant. *Zusatz:* Alle STEP-Formen eines Dokuments zusammen höchstens 200 MiB, schon beim Import geprüft (Meldung) und beim Speichern (eigene Meldung); sonst ließe sich ein Dokument zwar bearbeiten, aber nicht speichern.
- **P13 Fenster:** Wie „Netz importieren“. Das Ergebnis gilt nur für den Dokumentstand bei der Dateiwahl. Das Fenster beendet eine offene Skizze (`try_close_sketch`), bevor es aufgeht.
- **P14 Laden:** Beschädigter Körpertext → Fehlerstatus nur dieses Körpers. *Abweichung:* Ein beschädigter Kopf, Nicht-ASCII-Text oder eine Genauigkeit außerhalb 1e-6 … 1e-3 lehnt die **ganze Version** ab; die App fällt auf die nächstältere lesbare Version zurück.
- **P15 Fortschritt:** Zwei Aufträge nacheinander, je Auftrag ein neuer Fortschritt; Abbrechen in beiden.
- **P16 Nachziehen der Ecken:** Eingetreten, siehe Tor unten. Umgesetzt in `weld.rs`: Ecken werden innerhalb der Genauigkeit der Datei **genau** auf den Schnitt ihrer Flächen gezogen (auf etwa 1e-6 mm; weniger genau lehnt der Kernel schon bei 1e-5 ab).

### Torergebnis aus Aufgabe 4 (Machbarkeit fremder Körper im Kernel)
Frage: Rechnet der Kernel mit eingelesenen Körpern (Prüfen, Volumen, Abziehen eines Stabs, Verrunden, Schnitt)? Ergebnis: **Das Tor ist bestanden, kein Kernel-Eingriff nötig.**

| Körper | Prüfung | Volumen | Abziehen | Rundung | Schnitt |
|---|---|---|---|---|---|
| Quader (`box_ap214.step`), genau | ok | ok | ok | ok | ok |
| Ring mit Bohrung aus Halbschalen (`bore_halves.step`), genau | ok | ok | ok | ok | ok |
| Ring mit Bohrung, Zylinder mit Naht (`bore_seam.step`), genau | ok | ok | ok | ok | ok |
| dieselben drei, grob (Ecken um 9e-4 mm versetzt, Genauigkeit 1e-3 mm), **vor** Aufgabe 7 | ok | ok | Kernel lehnt ab: „… liegen zu knapp beieinander …“ | Kernel lehnt ab (Meldung zur Rundung) | Kernel lehnt ab |
| dieselben drei, grob, **nach** der Verschärfung in Aufgabe 7 (P16) | ok | ok | ok | ok | ok |

Das Volumen stimmt bei genauen Körpern auf 1e-4, bei groben auf 1e-3 (relativ, über das Netz mit Sehne 0,001 mm). Der Kernel rechnet nie mit falschem Ergebnis: Grobe Körper wurden vor der Verschärfung mit einer Meldung abgelehnt, nicht falsch gerechnet. Abweichung vom Wortlaut: Der Stab 4 × 4 × 100 geht beim Ring quer (entlang x in halber Höhe), weil ein Stab entlang z ganz in der Bohrung läge. Der Test prüft zusätzlich, dass der Parameterbereich jeder gekrümmten Fläche ihren Rand enthält, weil der Kernel einen verschobenen Bereich nicht bemerkt. Eine Ecke 2e-3 mm neben ihrer Kante (also über der Genauigkeit der Datei) wird weiter abgelehnt.

### Grenzen und angenommene Punkte
Alles unten ist bekannt, in den Reviews beurteilt und bewusst angenommen oder als eigener Auftrag vorgemerkt.

**Was das Programm nicht liest (in der Beschreibung für Nutzer: README, „STEP-Import“)**
- Flächenmodelle, offene Schalen, einzelne Flächen und Körper aus Dreiecken (`FACETED_BREP`): keine Körper; es gibt eine Meldung, welche Art in der Datei steht.
- Farben, Materialien, Ebenen, Toleranzen, Bemaßung (PMI), die Gliederung der Baugruppe, `.stpz`, XML-Form.
- `OFFSET_SURFACE`, `DEGENERATE_TOROIDAL_SURFACE`, Polylinie, Parabel, Hyperbel: Körper wird mit „… wird noch nicht unterstützt“ ausgelassen.
- **Torus und andere rundum gehende Flächen ohne Naht** (außer Zylinder, Kegel, Kugel): ausgelassen (P9). *Dennis hat am 2026-10-23 entschieden, dass dafür ein eigenes Teilprojekt aufgenommen wird (Abschnitt „Teilprojekt (aufgenommen): Torusbänder ohne Naht“ am Ende).*
- **Drehflächen aus einer schrägen Geraden:** ausgelassen mit eigener Meldung.
- **Speicherbedarf je Version:** Der Text steht in jeder gespeicherten Version erneut (grob 1 KB je Fläche); alle Formen eines Dokuments zusammen höchstens 200 MiB. Das Speichern bei voller Grenze braucht etwa 600 MB; die Grenzprobe braucht im Debug-Build 6 bis 8 s und 0,9 GB.

**Sicherheitsnetz des Lesers (Seitenprobe gegen verdrehte Körper)** – das Ziel „nie ein still anderer Körper“ ist erreicht, soweit eine Stichprobe es erreichen kann:
- Die Prüfung arbeitet mit Probepunkten (bis 512 Probepunkte je Schale und eine begrenzte Zahl Dreiecke je Fläche). Eine Datei mit Wänden dünner als etwa zwei Sehnen (0,1 mm; bei Körpern unter 50 mm etwa 2e-3 der Diagonale) wird in diesen Wänden **nicht** geprüft. Überlappungen oder Hohlräume, die weniger als etwa 0,1 mm aus dem Körper ragen, werden angenommen; tiefere nur, wenn ein Probepunkt hineinfällt. Nicht erkannt werden unter anderem Eckenüberlappungen, ein Hohlraum im Hohlraum und flache Überlappungen unter 0,3 mm.
- Ein Teil der Fehler in einer Datei beschreibt einen **anderen, gültigen Körper** und ist deshalb nicht erkennbar: ein umgedrehtes Richtungsbit an einer Kugelmulde oder -kappe (die Datei beschreibt dann genau die Vereinigung, die der Kernel selbst bilden würde), umgedrehte Kreiskanten bei Quader ⊕ Kugel (27 von 3015 gedrehten Fällen, die Kugel wird von der Kreisebene geschnitten), umgekehrte Mulde oder Kuppe, deren Rand durch beide Pole geht.
- Nach innen gekehrte Dateien (alle Flächenseiten verkehrt) mit einem Kugelstück, das genau **einen** Pol berührt oder dessen Seite nur aus der Schleifenrichtung folgt (zum Beispiel eine Eckrundung): ausgelassen mit „nicht eindeutig“.
- Der Vernetzer des Kernels scheitert bei einem Rand 0,02 mm neben einem Pol (1 von 12 Fällen; der Körper wird mit Meldung ausgelassen, nie falsch gebaut). Der Kernel tastet dort alle 5 Grad ab.
- Ein zufälliger Kernel-Körper (Quader minus Kugel, Zufallsreihe `Lcg(777)`) scheint selbst fehlerhaft zu sein: Sein Netz hat das Volumen des Quaders, und er wird in 3 von 4 Lagen abgelehnt. Der Fehler liegt wohl im Kernel, nicht im Leser.
- Eine Bohrung genau auf dem eingefügten Nahtwinkel wird von `validate` abgelehnt (bekannt).

**Genauigkeit und Zusammenziehen**
- Das Zusammenziehen ist bewusst **nicht transitiv** (Spec §4a): Kopien einer Ecke müssen innerhalb der Genauigkeit der Datei beieinander liegen. Dateien, bei denen die Kopien bis zu 2 × Genauigkeit auseinanderliegen, werden mit der Lücken-Meldung abgelehnt.
- Grobe Kreise und Ellipsen werden nicht ersetzt, Ecken und Kanten von Freiformflächen werden nicht bewegt, und dieselbe Kurve in zwei Darstellungen (Gerade und Grad-1-B-Spline) wird nicht gepaart. Eine Vollkreis-Kopie mit anderer Startecke wird abgelehnt. Koordinaten über etwa 1e9 mm werden mit einer Meldung abgelehnt. Die Lückenzahl nennt auch ungepaarte Endecken.
- Das Zusammenziehen prüft nicht auf Abbruch (linear, bei bis zu 20 000 Flächen kurz).
- Die Lücken- und Warnschwellen sind einmal gemessen (Tor), nicht für jede Geometrie bewiesen.

**Baugruppen**
- **Geprüft (Schlussprüfung):** Die Rechenregeln für `CARTESIAN_TRANSFORMATION_OPERATOR_3D` entsprechen der veröffentlichten EXPRESS-Fassung von ISO 10303-42 (`base_axis`, `first_proj_axis`, `second_proj_axis`; z = axis3 oder (0,0,1), x = axis1 senkrecht zu z projiziert, ohne axis1 (1,0,0) bzw. (0,1,0) bei z = ±x, y = axis2 projiziert, ohne axis2 (0,1,0); kein Kreuzprodukt, das System darf linkshändig sein). Abweichungen sind vorsichtig, immer mit Meldung und Körper am Ursprung: linkshändig → „Spiegelung“ (andere Programme drehen dort teils), Maßstab ≠ 1 → „Skalierung“, Achsen über den Kosinus 1e-4 schief oder unbestimmt → „schiefe Achsen“ (darunter wird wie in der Norm projiziert). Eine echte Datei eines anderen Programms wurde dafür nicht gelesen.
- Zwei Vorkommen derselben Produktdefinition bleiben getrennt; zwei `CONTEXT_DEPENDENT_SHAPE_REPRESENTATION` an einer Beziehung ergeben zwei Körper; Beziehungen mit Argumenten, die keine Verweise sind, werden übersprungen (in einer syntaktisch gültigen Datei nicht möglich).
- Unbekannte Teile in komplexen Einträgen werden still übergangen.
- Alle Proben von Baugruppen sind handgeschrieben (`inch_assembly.step`, `mapped_item.step`) oder mit dem eigenen Schreiber erzeugt. **Es wurde keine Datei eines anderen CAD-Programms gelesen** (Lizenz: keine fremden Beispiele im Repository).

**Dauer und Speicher**
- **Laden einer Projektdatei mit STEP-Körpern** baut die Körper in der Oberfläche auf (Fenster steht kurz). Seit der Schlussprüfung wächst die Sehne der Seitenprobe und der Hülle ab 100 mm Größe mit (5e-4 der Diagonale): Vase ×1 / ×100 / ×1000 Neuladen 0,07 / 0,12 / 0,12 s (vorher 0,08 / 0,57 / 8,4 s), Import-Prüfung 0,17 / 0,35 / 0,34 s (vorher 0,17 / 1,5 / 23 s). Dafür bleiben bei großen Körpern Wände unter zwei Sehnen ungeprüft (10 m: etwa 10 mm). Abbrechen wirkt weiter nur zwischen Stufen und Flächen, nicht innerhalb eines Netzes (der Vernetzer hat keinen Abbruch). Das Importieren selbst läuft im Hintergrund. Eine spätere Aufgabe könnte das Laden im Hintergrund erledigen.
- Gültige, aber ungünstige Körper brauchen auch im Release-Build Zeit: eine gültige „Vase“ mit 230 000 Dreiecken 3,7 s, eine Kugel mit Bohrung 1e-4 … 1e-2 mm 2,0 bis 3,4 s (der alte Code ebenso); die Flanschdatei der feindlichen Proben 1,5 s Release (11 s Debug, deshalb nur im Release-Test mit Zeitgrenze).
- Grobe Ringe vernetzen langsam (im Debug-Build bis 3 s bei Sehne 0,001).

**Fenster und Dokument**
- **„Bearbeiten …“** ist für STEP-Körper (wie für Netz-Körper) sichtbar und ohne Wirkung. Ein eigener Aufräum-Auftrag könnte den Eintrag ausgrauen.
- **Einheit je Körper:** `planora-step` kennt noch kein eigenes Einheiten-Feld je Körper. Die App findet die Einheit jeder Zeile mit einer zweiten Suche in der Datei (Anzahl und Namen müssen passen) und schreibt sonst „gemischt“. Bei gemischten Dateien stellt eine Wahl **alle** Kontexte um; der Hinweis im Fenster sagt es.
- Die Größen in der Liste sind auf 0,1 mm gerundet und bei gekrümmten Flächen ungefähr (Netz mit Sehne 0,05 mm; die Hülle ist bis zu 0,05 mm zu klein). Der Text einer Fehlermeldung des Betriebssystems beim Lesen der Datei steht unverändert da. Lange Listen rollen als Ganzes.
- Warnungen zu einzelnen Körpern (Lage am Ursprung) stehen nur **vor** dem Import in der Liste; danach zeigt das Eigenschaftenfeld nur die grobe Genauigkeit.
- **Keine Größengrenze vor dem Lesen der Projektdatei** (gilt auch für Netze, Format 3): Versionsdateien und Autosicherung werden ganz gelesen, bevor die 200-MiB-Summe zählt. Vorschlag: eigene Aufgabe, die Dateigröße vor `read_to_string` zu begrenzen (etwa 2 × Summengrenze plus Rahmen).
- Die Grenzprobe für die Summe benutzt einen nur für Tests gedachten Haken (`#[doc(hidden)]`) im Produktionscode.

**Tests (bewusst angenommene Lücken)**
- Das Netz zweier Körper mit gleicher Geometrie, aber verschiedener Parametrisierung weicht leicht ab (4 von 60 Fällen bis 4,5e-4 im Volumen und 7e-3 mm in der Hülle), weil der Netzer vom Parameterbereich abhängt. Der Round-Trip-Test prüft darum die Geometrie selbst (Punkte jeder Fläche auf einer Fläche des anderen Körpers, Kantenpunkte, Ecken; je 1e-9 bzw. `NURBS_EDGE_TOL`) und das Netz nur bis zu seinem eigenen Fehler.
- Das Abziehen eines Stabs fehlt bei Körpern mit Kegel- oder Torusfläche (der Kernel lehnt „Schnitt von Kegel/Torus mit Ebene“ auch bei eigenen Körpern ab); das Rundung-Probe läuft nur dort, wo der Kernel auch das Original rundet.
- Die Zufallsfälle der feindlichen Dateien zählen über alle Dateien zusammen, nicht je Datei; das Zeitlimit je Datei prüft nur der Release-Test.
- Einige Mutationen blieben begründet grün (Absicherungen doppelt vorhanden, gleichwertige Prüfungen, nicht bestimmbare Zufälle). Die Liste je Aufgabe steht im Bericht, ebenso die Gründe.
- `planora-project` darf nicht von `planora-step` abhängen; dessen Tests benutzen darum handgeschriebene STEP-Quader. Der Aufbau anderer Flächenarten ist in `planora-step` und `planora-feature` getestet.

### Zahlen
- **Mutationsproben der Aufgaben 1 bis 14: 605** (einzeln mit dem Editor oder Skript eingespielt, mit den Originalbytes zurückgenommen, Hash geprüft; Quelle: die Berichte in `.superpowers/sdd/2026-10-21-step-import/task-N-report.md`, alle Runden einschließlich der Nacharbeiten gezählt). Je Aufgabe: 1: 64 · 2: 45 · 3: 41 · 4: 17 · 5: 41 · 6: 101 · 7: 30 · 8: 51 · 9: 64 · 10: 19 · 11: 28 · 12: 28 · 13: 31 · 14: 45. **573 rot, 32 grün geblieben**, alle 32 mit Begründung (keine offene grüne): gleichwertig (Prüfung doppelt vorhanden oder ohne Wirkung auf das Ergebnis, z. B. Aufgabe 4: M9, M14; Aufgabe 6: M29, M30, N4), „Absicherung“ (eine zweite Prüfung fängt denselben Fall), nur mit unvertretbarem Aufwand oder nicht deterministisch tötbar (Aufgabe 6: M06, M09, M11, W12, W13, W15, W18) oder Testlücke ohne Wirkung auf gültige Dateien. Einige Mutationen blieben zuerst grün und wurden durch einen schärferen Test rot (nicht in den 32 gezählt). Aufgabe 8 zählt die 20 Mutationen der ersten Runde nur einmal, obwohl die Fix-Runde sie alle noch einmal rot laufen ließ; Aufgabe 10 ebenso (M01, M15, M16). Nicht gezählt: die Einzelproben der Prüfer (zwischen 16 und 44 je Aufgabe, z. B. Aufgabe 4: 16, Aufgabe 8: 44, Aufgabe 13: 28, Aufgabe 14: 24); sie liegen in den `task-N-review.md`.
- **Gates (Abschlussprüfung am 2026-10-23, Stand `a17497c`):** `cargo fmt --all --check` sauber. `cargo clippy --workspace --all-targets -- -D warnings` sauber. `cargo test --workspace -j 2 --no-fail-fast`: 3393 bestanden, 0 fehlgeschlagen, 47 ausgeschlossen (`#[ignore]`) in 329 Testläufen, rund 23 Minuten im Debug-Build einschließlich Übersetzen. `cargo test -p planora-step --release -- --ignored`: 6 Tests grün (`read_hostile` 3 Tests 26,2 s, `roundtrip` 1 Test 20,7 s, `stage3` 2 Tests 3,6 s). `cargo deny check`: advisories, bans, licenses, sources ok; die Ausnahmen für `ttf-parser` (RUSTSEC-2026-0192) und die zbus-Familie (RUSTSEC-2026-0370, -0371, -0372) greifen unverändert (keine Meldung „nicht gefunden“), `deny.toml` ohne Diff. Die Kernel-Crates ohne Diff gegen `c16d988`.
- Keine neue externe Abhängigkeit; zwei interne Kanten (siehe oben).

### Handliste für den Nutzer (Prüfung von Hand im echten Fenster)
Release-Build, Fenster zuerst 1400 × 900. Dateien: Kopien der Proben aus `crates/planora-step/tests/data/` in einen Ordner außerhalb des Repositorys (`box_ap214.step` ist ein Quader „Würfel“, `bore_halves.step` und `bore_seam.step` sind ein „Ring“ mit Bohrung, `inch_assembly.step` ist eine Baugruppe in Zoll mit „Gestell“, „Block“ und „Stift“). Dazu ein Export eigener Körper (Treppe mit vielen Flächen) und eine selbst abgeänderte Kopie von `box_ap214.step`, in der `LENGTH_MEASURE(1.E-07)` durch `LENGTH_MEASURE(1.E-03)` ersetzt ist (grobe Genauigkeit). Jeweils prüfen: Texte lesbar, nichts ragt heraus, Meldung nennt den Grund. Abhaken:

**A. Einlesen und Fenster**
- [ ] 1. „Datei → STEP importieren …“ (direkt unter „Netz importieren …“, Symbol vorhanden) öffnet das Fenster „STEP importieren“ mit „Datei wählen (STEP)“. Dateiauswahl, `box_ap214.step` wählen: Fortschrittsbalken „Die Datei wird gelesen …“, dann „Die Körper werden geprüft …“, dann die Liste mit einem Körper „Würfel“ (Spalten Name, Flächen = 6, Größe, Einheit, Status „wird übernommen“), Häkchen gesetzt.
- [ ] 2. „Importieren“: Meldung „1 Körper importiert …“ (in neutraler Farbe, nicht rot), Kamera zeigt den Quader, er ist gewählt, der Baum zeigt „Würfel“ mit Symbol, das Eigenschaftenfeld zeigt Flächen, Kanten und die Genauigkeit der Datei.
- [ ] 3. Ring mit Bohrung (`bore_halves.step`, dann `bore_seam.step`): Liste, Importieren; Bohrung und Wand stimmen (Querschnitt prüfen, siehe Punkt 9).
- [ ] 4. Baugruppe `inch_assembly.step`: Die Liste zeigt „Block“, „Block (2)“, „Stift“ (das Gestell selbst hat keinen Körper), Einheit „Zoll“; die Größen sind das 25,4-fache der Zahlen in der Datei.
- [ ] 5. Einheit auf „Millimeter“ stellen und zurück auf „Zoll“: Die Größen ändern sich, die Häkchen bleiben. Nach „Millimeter“ sind alle Größen 25,4-mal kleiner.
- [ ] 6. Importieren der Baugruppe: Meldung „3 Körper importiert“; die Körper liegen so wie in der Datei zueinander (nicht alle am Ursprung). Strg+Z nimmt alle drei zurück (ein Schritt, Name „STEP importieren“), Strg+Y bringt sie wieder.

**B. Weiterverwenden**
- [ ] 7. Block mit einem Zylinder abziehen; eine Kante runden; eine Skizze auf eine Blockfläche („STEP-Fläche 3“ in der Auswahl) legen und extrudieren. Alles gelingt, oder die Meldung nennt klar den Grund.
- [ ] 8. Importierten Körper verschieben (Position ziehen oder Verschieben-Feature).
- [ ] 9. Zeichnung mit Draufsicht und Schnitt durch den Ring; Stückliste mit den Importen: Namen und Zahl stimmen.
- [ ] 10. STEP-Export des importierten Körpers und erneuter Import: gleiche Flächenzahl.

**C. Speichern und Öffnen**
- [ ] 11. Speichern, Tab schließen, Dokument öffnen: Körper, Namen, Lage und Marken sind gleich; die Skizze aus Punkt 7 sitzt auf derselben Fläche. Das Öffnen dauert bei einem großen gekrümmten Körper kurz (siehe Grenzen).
- [ ] 12. Ein Dokument ohne STEP-Körper speichern: Dateiformat und Inhalt wie vorher (nicht Format 10).
- [ ] 13. Eine von Hand beschädigte Datei: ein Zeichen im STEP-Text einer Form ändern, Dokument öffnen: Es lädt, der Körper zeigt den Fehler im Baum. Den Kopf des Textes beschädigen: die Version wird abgelehnt und die vorherige geöffnet.

**D. Warnungen, Auslassen, Abbrechen**
- [ ] 14. Grobe Datei (Kopie mit `1.E-03`): In der Liste steht „wird übernommen – … Rechnen mit diesem Körper kann scheitern“, nach dem Import steht dieselbe Warnung im Eigenschaftenfeld. Abziehen und Runden gelingen oder melden klar.
- [ ] 15. Datei mit ausgelassenem Körper (zum Beispiel eine selbst abgeänderte Kopie, in der eine Ebene durch `OFFSET_SURFACE` ersetzt ist): roter Grund „wird ausgelassen: …“ in der Liste, Häkchen gesperrt, die übrigen Körper lassen sich importieren.
- [ ] 16. Abgeschnittene Datei (bei der Hälfte abschneiden): Meldung mit Zeilennummer, keine Abstürze. Keine STEP-Datei (Textdatei umbenannt): Meldung „… keine STEP-Datei …“.
- [ ] 17. Große Datei (Export einer Treppe mit 10 000 Flächen): Die Oberfläche bleibt bedienbar, „Abbrechen“ wirkt sofort, danach lässt sich eine andere Datei wählen.
- [ ] 18. Datei mit mehr als 50 Körpern (Export von 51 Quadern): „Die Datei enthält mehr als 50 Körper.“
- [ ] 19. Dokument ändern (zum Beispiel einen Quader anlegen), während die Liste offen ist, dann „Importieren“: nichts wird angelegt, die Meldung bittet, die Datei noch einmal zu wählen.
- [ ] 20. „Schließen“ und „Abbrechen“ während des Lesens: Das Fenster schließt, nichts bleibt im Hintergrund hängen. Eine offene Skizze: sie wird beim Öffnen des Fensters beendet.

**E. Fenster, Tasten, Aussehen**
- [ ] 21. Enter im Feld „Dateiname“ der Dateiauswahl öffnet die Datei; „Andere Datei wählen …“ geht; „Abbrechen“ in der Dateiauswahl lässt das Fenster mit der alten Liste offen.
- [ ] 22. Liste mit 50 Körpern: „Importieren“ bleibt erreichbar (Fußzeile fest), die Liste rollt; keine Zeilen wrappen.
- [ ] 23. Fenster 1100 × 700 und 800 × 600: Das Fenster, die Liste und die Fußzeile sind im Bild, nichts wird abgeschnitten.
- [ ] 24. Hell und Dunkel: Symbol im Menü, Status „wird übernommen“ / „wird ausgelassen“ und die Warnung sind lesbar, Farben stimmen mit dem übrigen Programm; Hinweise (Einheit, gemischt) nicht rot.
- [ ] 25. Strg+Z und Strg+Y bei offenem Fenster tun nichts am Dokument; nach „Schließen“ wirken sie wieder.

## Teilprojekt (aufgenommen, noch nicht begonnen): Torusbänder ohne Naht beim STEP-Import

Entscheidung von Dennis am 2026-10-23: Der STEP-Import soll auch Körper lesen, deren Torusfläche (Rundung zwischen zwei Kreisen, Rohrbogen, Reifen) als rundum gehendes Band ohne Naht in der Datei steht. Bisher wird so ein Körper mit Begründung ausgelassen (P9), die übrigen Körper der Datei kommen an.

- **Ausgangslage:** Zylinder, Kegel und Kugel ohne Naht liest der Leser (Naht wird beim Lesen eingefügt, `seam.rs`: Naht mit Abstand zu allen anderen Rändern, Pole geometrisch erkannt). Der Algorithmus mit zwei Kreisen lässt sich auf den Torus übertragen; beim Torus kann die Fläche in beiden Richtungen rundum gehen (zwei Nähte, u und v).
- **Zu klären:** Welche Formen echte Programme schreiben (Rundung zwischen zwei Kreisen, Rohrbogen mit zwei Rohrkreisen, voller Reifen mit zwei Kreisen), wie die Seitenprobe (`check_face_sides`) und die Wickelregel (Rand schließt sich nicht, Aufgabe 3) mit zwei Nähten umgehen, und welche Gegenproben gebraucht werden.
- **Vorgehen:** wie bei den anderen Teilprojekten: Entwurf, Plan, Bau mit Prüfungen, Handtest. Dazu gehört, mindestens eine echte STEP-Datei eines anderen CAD-Programms zu bekommen (bisher wurden nur Dateien aus dem eigenen Schreiber und von Hand gebaute Proben gelesen).

## Teilprojekt: Dateiauswahl

Entscheidung von Dennis am 2026-10-23; gilt für alle drei Fenster mit Dateiauswahl (Netz importieren, STEP importieren, Exportieren), weil sie dieselbe Auswahl nutzen.

**Geliefert (Zweig `feature/dateiauswahl`):**
- Ordnersymbol (Phosphor `folder`, Markierungsfarbe des Themas) statt „(Ordner, Doppelklick zum Öffnen)“; der Hinweis „Doppelklick zum Öffnen“ steht nur noch im Tooltip. Dateien haben ein graues Dateisymbol (Phosphor `file`), damit die Namen bündig stehen. Ordner öffnen bleibt Doppelklick.
- Schnellzugriffe über der Liste (brechen in weitere Zeilen um): Dokumente, Downloads, Desktop, Bilder (im Benutzerordner, sonst unter `OneDrive`, fehlende werden nicht gezeigt), Projekte (gewählter Projektordner oder die Vorgabe, nur wenn vorhanden), die Laufwerke, dann die Favoriten. Der Knopf des aktuellen Ordners ist hervorgehoben; ein Klick wechselt wie ein eingetippter Ordner.
- Laufwerke werden einmal je Programmlauf im Hintergrund gesucht (je Buchstabe ein eigener Thread); ein hängendes Netzlaufwerk hält die Oberfläche nicht auf, es erscheint erst, wenn es antwortet.
- Favoriten: Rechtsklick auf einen Ordner der Liste → „Zu Favoriten hinzufügen“ bzw. „Aus Favoriten entfernen“; Rechtsklick auf einen Favoriten-Knopf → „Aus Favoriten entfernen“. Höchstens 24, ohne Doppelte (Groß- und Kleinschreibung und Schrägstrich am Ende zählen nicht), Reihenfolge des Hinzufügens. Gleich heißende Favoriten zeigen den übergeordneten Ordner mit. Ein Favorit, dessen Ordner fehlt, bleibt sichtbar; ein Klick meldet das und nennt den Weg zum Entfernen.
- Gespeichert in `config.json` als Feld `favorites` (fehlt es in alten Dateien, bleiben Wurzel und alles andere erhalten; ältere Programmstände übergehen das Feld). Speichern gleich nach jeder Änderung; ein Fehler beim Schreiben ist nicht schlimm.
- Nachbesserung 1 (nach der Prüfung):
  - Die Leiste bricht jetzt wirklich um und rollt ab zwei Zeilen; die drei Fenster bleiben bei 800 × 600 auch mit 24 Favoriten (auch mit langen Namen) höchstens 720 px breit, „Speichern“ bzw. „Öffnen“ ist sichtbar.
  - Lesen, ändern, schreiben: Ein Favorit ändert nur die Favoriten in der Datei (die Änderung wird auf den Stand auf der Platte angewandt), die Wahl der Wurzel ändert nur `root`. So überschreibt eine zweite Instanz oder eine beim Start kurz unlesbare Datei weder Wurzel noch fremde Favoriten. Eine unlesbare Datei wird durch Favoriten nie überschrieben.
  - `favorites` wird nachsichtig gelesen: `null`, ein falscher Typ, leere oder nicht-textliche Einträge werden übergangen, Doppelte und mehr als 24 beim Laden entfernt; die Wurzel lädt immer.
  - Die Konfiguration schreibt über eine eigene Zwischendatei je Schreibvorgang.
  - Dokumente mit Symbol `files`, Projekte mit `briefcase` (deutlich anders als die Ordnerzeilen).
  - Neuzeichnen während der Laufwerkssuche höchstens 3 s, danach zeigt die Auswahl die bis dahin gefundenen Laufwerke.
- Nachbesserung 2 (nach der Nachprüfung):
  - „Speichern“/„Ersetzen“ bzw. „Öffnen“ und „Abbrechen“ samt Meldungen stehen in allen drei Fenstern in der festen Fußzeile: auch in einem vollen Ordner bei 800 × 600 ohne Rollen sichtbar. Beim Netz-Import steht „Importieren“ ebenfalls in der Fußzeile. Braucht die Leiste eine zweite Zeile, wird die Liste um diese kürzer (die Auswahl wird nicht höher).
  - Favoriten-Änderungen, die nicht gespeichert werden konnten (Datei unlesbar, schreibgeschützt, gesperrt), bleiben in der Leiste und vorgemerkt; sie werden beim nächsten gelungenen Schreiben (Favorit oder Wurzelwahl) mit gespeichert.
  - Ist die Liste in der Datei schon voll (andere Instanz), meldet die Auswahl „Es gibt schon 24 Favoriten …“, statt den neuen Favoriten still fallen zu lassen.
- Nachbesserung 4: Bei der Dateiauswahl gibt es in allen drei Fenstern nur „Öffnen“/„Speichern“ und „Abbrechen“, kein zusätzliches „Schließen“ (das ✕ oben bleibt). „Abbrechen“ schließt das Fenster, wenn es nichts gibt, wohin es zurückgehen kann; STEP mit gelesener Liste kehrt zur Liste zurück, Exportieren zum Exportformular.
- Nachbesserung 3 (Rückmeldung von Dennis am laufenden Programm):
  - Alle Zeilen (Kopfzeile mit Schließen-Symbol am rechten Rand, Ordnerzeile, Leiste, Liste, Dateiname, Fußzeile) nutzen dieselbe Breite. Ursache des zu breiten „Netz importieren“: sehr lange Datei- oder Ordnernamen machten die Listenzeile breiter als das Fenster vorsah, das Fenster wuchs mit. Jetzt geht jede Listenzeile über die ganze Breite, zu lange Namen enden mit „…“. Knöpfe der Leiste zeigen höchstens 40 Zeichen (voller Pfad im Tooltip).
  - Überfahren, Drücken oder Hervorheben eines Knopfs der Leiste ändert seine Größe nicht mehr (gleicher Rahmen in jedem Zustand), die Leiste verschiebt sich nicht.
  - Die drei Dateifenster öffnen immer mittig im Programmfenster, auch nach einer Größenänderung des Programmfensters zwischen zwei Öffnungen (danach verschiebbar wie bisher). Sie sind 55 % der Fensterbreite breit (720 bis 1200 px); die Liste wächst mit der Fensterhöhe (bei 600 px wie bisher, höchstens 700 px). Bei 800 × 600 ändert sich nichts.

**Grenzen:**
- Die bekannten Ordner werden über die üblichen Namen im Benutzerordner gefunden (`Documents`, `Downloads`, `Desktop`, `Pictures`, ggf. unter `OneDrive`), nicht über die Windows-Einstellung für verschobene Ordner (z. B. `OneDrive\Dokumente` wird nicht gefunden).
- Laufwerke, die erst während des Programmlaufs dazukommen (USB-Stick), erscheinen erst nach einem Neustart; ein Laufwerk, das länger als 3 s hängt, erscheint erst beim nächsten Bild nach seiner Antwort (zum Beispiel bei Mausbewegung).
- Favoriten lassen sich nur mit der Maus (Rechtsklick) hinzufügen und entfernen, nicht über die Tastatur.
- Im UI-Thread laufen weiterhin kurze Prüfungen: ob der Projektordner vorhanden ist (einmal je Auswahl), die bekannten Ordner (beim Öffnen der Auswahl) und das Öffnen eines Ordners per Knopf; ein hängendes Netzlaufwerk kann dabei kurz aufhalten (wie beim Eintippen).
- Ist die Konfigurationsdatei unlesbar, gilt wie bisher die Vorgabe; neue Favoriten bleiben vorgemerkt und werden gespeichert, sobald die Datei wieder lesbar und schreibbar ist, sonst gelten sie bis Programmende. Eine Meldung, dass das Speichern gescheitert ist, gibt es nicht.
- Eine von Hand mit BOM oder als UTF-16 gespeicherte config.json gilt als unlesbar (wie vor dem Teilprojekt).
- Stürzt das Programm genau beim Schreiben ab, kann eine Zwischendatei `config.json.<Nummer>.tmp` liegen bleiben (harmlos).
- Ändern zwei Programmfenster im selben Augenblick Favoriten, kann eine der beiden Änderungen verloren gehen (nie die Wurzel).
- Im Exportfenster gibt es bei 800 × 600 weiterhin bis zu drei Rollbereiche (Fensterinhalt, Leiste, Liste).

**Handtest (Hell und Dunkel, Fenster 1100 × 700 und 800 × 600):**
- [ ] 1. „Netz importieren …“: Ordner mit blauem Ordnersymbol, Dateien mit grauem Dateisymbol, Namen bündig; kein Text „(Ordner, …)“. Beim Überfahren eines Ordners erscheint „Doppelklick zum Öffnen“.
- [ ] 2. Einfacher Klick auf einen Ordner öffnet ihn nicht; Doppelklick öffnet ihn.
- [ ] 3. Knöpfe über der Liste: Dokumente, Downloads, Desktop, Bilder, Projekte, Laufwerke (C:, D: …). Jeder Klick wechselt dorthin, der Knopf des aktuellen Ordners ist hervorgehoben. Bei 800 × 600 brechen die Knöpfe um, das Fenster rollt.
- [ ] 4. Rechtsklick auf einen Ordner → „Zu Favoriten hinzufügen“: Der Ordner erscheint als Knopf mit Stern. Rechtsklick auf denselben Ordner zeigt jetzt „Aus Favoriten entfernen“. Rechtsklick auf eine Datei zeigt kein Menü.
- [ ] 5. Favorit in „STEP importieren …“ und in „Exportieren“ (Dateiauswahl) ebenfalls da; nach Programmneustart noch da.
- [ ] 6. Zwei Ordner gleichen Namens (z. B. `Kunde A\Teile` und `Kunde B\Teile`) als Favoriten: Die Knöpfe unterscheiden sich.
- [ ] 7. Favoriten-Ordner im Explorer umbenennen, dann Knopf klicken: Meldung, dass es den Ordner nicht gibt, mit Hinweis zum Entfernen; Rechtsklick auf den Knopf → „Aus Favoriten entfernen“ wirkt.
- [ ] 8. Mit einem getrennten Netzlaufwerk: Die Auswahl öffnet sofort, das Laufwerk fehlt oder erscheint später.
- [ ] 9. Ältere `config.json` ohne `favorites` (vorher sichern): Programm startet mit dem gewählten Projektordner.
- [ ] 10. Alle drei Dateifenster öffnen mittig, auch nachdem das Programmfenster zwischen zwei Öffnungen vergrößert oder verkleinert wurde; im großen Fenster (z. B. 2000 × 1100) sind sie breiter und die Liste höher, „Speichern“/„Öffnen“ bleibt unten sichtbar.
- [ ] 11. Ordner mit sehr langen Namen: Kopfzeile, Ordnerzeile, Leiste, Liste und Dateiname enden am selben rechten Rand, lange Namen enden mit „…“.
- [ ] 12. Maus über die Knöpfe der Leiste führen und einen drücken: kein Knopf verschiebt sich, nichts springt in die nächste Zeile.
- [ ] 13. Während die Dateiauswahl zu sehen ist, gibt es nur „Öffnen“ bzw. „Speichern“ und „Abbrechen“ (kein zusätzliches „Schließen“). „Abbrechen“ schließt „Netz importieren“ und „STEP importieren“ (ohne gelesene Liste); hat STEP schon eine Liste, führt es zu ihr zurück (dort wieder „Schließen“); beim Exportieren zurück zum Exportformular.

## Teilprojekt: Projekte-Fenster und Einstellungen

Entscheidungen von Dennis am 2026-10-23. Zweig `feature/projekte-fenster`. Entwurf: `docs/superpowers/specs/2026-10-23-projekte-fenster-design.md`, Plan mit „Abweichungen bei der Umsetzung“: `docs/superpowers/plans/2026-10-23-projekte-fenster.md`. Geändert hat sich nur `planora-app`, dazu kommt die neue Abhängigkeit `chrono` (siehe unten). Dateiformate und alle anderen Crates sind unberührt.

**Geliefert:**
- Menü „Datei → Projekte …“ (Strg+O, ersetzt „Öffnen …“) und „Datei → Einstellungen …“. Das Feld „Projekte“ links und „Ansicht → Projekte“ sind weg; links steht nur noch der Feature-Baum.
- Fenster „Projekte“ (nicht modal, mittig, etwa 55 % der Fensterbreite, 720 bis 1200 px): Kacheln mit Miniatur, „Geändert …“ in Ortszeit und „Version n“, zwei Ebenen (Projekte, dann Ordner und Dokumente), Doppelklick, Rechtsklick (Umbenennen, Verschieben, Löschen wie bisher), „Neues Projekt“, „Neuer Ordner“, „Neues Dokument“, Tab und Enter, „Aktualisieren“. Alle Aktionen des alten Felds sind von dort erreichbar.
- Fenster „Einstellungen“: „Projektverzeichnis ändern …“ öffnet die Dateiauswahl im neuen Ordner-Modus (mit Schnellzugriffen und Favoriten). Der Wechsel wird abgelehnt, solange ein gespeichertes Dokument offen ist. Es wird nichts verschoben, kopiert oder gelöscht; `config.json` behält Favoriten, nur `root` ändert sich.
- Miniaturen: eigener Software-Rasterer (kein Zugriff auf die Grafikkarte), ein Hintergrund-Arbeiter, Zwischenspeicher `vorschau.png` und `vorschau.json` im Dokumentordner, Absturzsicherung, Zeit- und Größengrenzen.
- Zeiten in Ortszeit (Kacheln und Reiter „Versionen“); gespeichert wird weiter UTC.

**Entscheidungen:**
- a) „Neues Dokument“ legt das Dokument sofort an (Version 1, Byte für Byte wie das erste Speichern), öffnet einen Tab und zeigt die Kachel. Strg+N bleibt wie es war.
- „Öffnen …“ wird zu „Projekte …“ (Strg+O bleibt). Texte, die auf das Feld verwiesen, sagen „im Fenster Projekte“. Ohne Projektverzeichnis lautet die Meldung: „Es ist noch kein Projektverzeichnis gewählt. …“
- Das Datum auf der Kachel ist die Zeit der Spitzenversion des aktiven Zweigs, nicht die der neuesten Version über alle Zweige. So gehören Datum und „Version n“ zur selben Version. Bei Projekten und Ordnern zählt das zuletzt geänderte lesbare Dokument darunter, bei Gleichstand der kleinere Name. Das weicht vom Wortlaut des Entwurfs (§ 4) ab.
- Ortszeit mit der Bibliothek `chrono` (nur die Uhr-Funktion; Entscheidung B von Dennis). Neu in `Cargo.lock`: `chrono`, `iana-time-zone`, `iana-time-zone-haiku`. `deny.toml` ist unverändert.
- Miniaturen: `art` in `vorschau.json` kann `bild`, `leer`, `zu_aufwendig`, `baut` oder `fehlgeschlagen` sein. Während eines Aufbaus steht eine Marke `baut` mit `pid`, `since` und `versuche`. Ist sie älter als 15 Minuten, gilt der Aufbau als abgestürzt: beim ersten Mal wird noch einmal versucht, beim zweiten Mal gilt `fehlgeschlagen` (Kachel: „Miniatur nicht möglich“) bis zur nächsten Version. Eine frische Marke heißt: ein anderes Programm baut gerade, es wird übersprungen (Kachel: „Wird gerade erzeugt …“).
- Es gibt genau eine Miniatur je Dokument (aktuelle Version, aktuelles Design). Beim Umschalten von Hell und Dunkel wird die andere gelöscht und neu gemalt.
- Grenzen für Miniaturen: höchstens 200 000 Dreiecke, höchstens 3 s Aufbau (nur zwischen zwei Körpern geprüft), eine Aufwandsgrenze fürs Malen (`render_cost`, 20 Millionen), Bild höchstens 1024 Punkte je Seite und 524 288 Punkte insgesamt. Darüber steht „Miniatur zu aufwendig“ (`zu_aufwendig`).
- Beim Beenden wartet das Programm höchstens 2 s auf den Arbeiter und räumt seine eigene Marke weg.
- Fenster schließen einander: Strg+O bei offenem Import schließt den Import; Import oder Export öffnen schließt „Projekte“. Ein schreibender Export wird nie geschlossen; dann öffnen „Projekte“ und „Einstellungen“ nicht, und die Statuszeile sagt „Bitte erst den Export beenden.“
- Steht eine Rückfrage auf dem Bildschirm (zum Beispiel nach Strg+S oder „Dokument gesperrt“), ist „Projekte“ ganz gesperrt, und „Einstellungen“ lassen sich nicht öffnen oder ändern. Sonst könnte das Verzeichnis unter der Rückfrage wechseln. Auch die Tasten für Rückgängig sind bei offenen Einstellungen und Eingabefenstern gesperrt.
- Namen von Dokumenten dürfen nicht auf `.pcad` enden (bei „Neues Dokument“, „Speichern unter“ und „Umbenennen“, wie bei Ordnern).
- Abschlussprüfung: Speichern, Wiederherstellen und Zweigwechsel erneuern die Kacheln bei offenem Fenster sofort (Ort, Auswahl und offene kleine Fenster bleiben). Ein leeres Projekt oder ein leerer Ordner sagt das in einem ruhigen Hinweis. „Speichern unter“ ohne Projekt bzw. ohne Projektverzeichnis rät, mit „Abbrechen“ zu schließen und dann Strg+O bzw. „Datei → Einstellungen …“ zu nehmen (bei offener Rückfrage sind beide Fenster gesperrt). Die neuen Texte duzen durchgehend („Deine Projekte bleiben, wo sie sind. …“) und sagen „Projektverzeichnis“; nur die Meldung von `choose_root` („… bevor der Projektordner wechselt.“) bleibt wörtlich, wie der Plan es verlangt.

**Grenzen:**
- Das Datum im Schriftfeld und die Blattdaten der Zeichnung (`Stamp::now` in `planora-drawing`) sind weiter der UTC-Tag. Zwischen Mitternacht und 1 bis 2 Uhr Ortszeit zeigen sie den Vortag. Das ist ein eigener Punkt, nicht Teil dieses Teilprojekts.
- Die Tests zur Ortszeit mit Berliner Zeit prüfen nur auf Rechnern mit dieser Zeitzone alles.
- Im Programm selbst laufen weiter kurze Prüfungen, ob Ordner da sind (Wechsel des Verzeichnisses, Öffnen der Auswahl). Ein hängendes Netzlaufwerk kann dabei kurz aufhalten.
- Konnte `config.json` beim Wechsel des Projektverzeichnisses nicht geschrieben werden, gilt der Wechsel trotzdem für diese Sitzung; das Programm warnt, dass er nicht gemerkt wird. Intern kommt das als Fehler zurück, obwohl gewechselt wurde.
- Felder in `config.json`, die dieses Programm nicht kennt, gehen beim Schreiben verloren (war schon vorher so; `planora-project` ist eingefroren und bleibt unverändert). Beheben, sobald `planora-project` wieder geändert werden darf (unbekannte Felder mitführen).
- Der Aufbau eines großen Dokuments für die Miniatur lässt sich nicht abbrechen. Das Fenster bleibt bedienbar, aber die übrigen Miniaturen kommen erst danach.
- Die Miniatur entsteht erst, wenn das Fenster geöffnet wird, nicht schon beim Speichern. Danach liegt sie auf der Platte.
- Es gibt nur eine Miniatur je Dokument: Wechselst du oft zwischen hell und dunkel, wird jedes Mal neu gemalt.
- Ohne Schreibrecht im Dokumentordner gibt es keine Absturzsicherung für die Miniatur (sie bleibt nur im Speicher). Eine Marke kann fälschlich auf „fehlgeschlagen“ führen, wenn das Speichern und das Wegräumen der Marke beide scheitern; auch Prozessnummern können sich wiederholen. Beim Beenden mitten im Aufbau bleibt in seltenen Fällen die Marke liegen (kurzes Zeitfenster).
- Höchstens 64 Bilder liegen gleichzeitig im Grafikspeicher (nur Kacheln im und nahe am sichtbaren Ausschnitt); entferntere werden bei Bedarf neu geholt.
- Höchstens 500 Kacheln je Ebene: Das Programm liest alle Dokumente, zeigt aber nur die ersten 500 („500 von 600 Projekten“).
- Das schlimmste Malen einer Miniatur dauerte in der Debug-Version etwa 9 s (ein Stab mit 100 000 Segmenten). Die Aufwandsgrenze verhindert das jetzt; Fälle knapp darunter dauern weiter lange, halten aber nur den Arbeiter auf. PNG-Dateien werden beim Lesen auf ihre Größe geprüft, beschädigte gelten als fehlend.
- Die ausgewählte Kachel hat in beiden Designs eine leicht blau getönte Fläche (`accent_subtle`) und einen 2 px breiten blauen Rand innen; im hellen Design hebt sie sich so auch vom weißen Fenster ab (Kontrast etwa 1,26 : 1, nur eine Fläche, kein Text).
- Umbenennen, Verschieben oder Löschen kann kurz mit „Zugriff verweigert“ scheitern, während der Miniatur-Arbeiter oder das Lesen gerade eine Datei in diesem Ordner offen hat. Es geht nichts verloren (ein einziger Schritt, ein Hinweis erscheint); ein zweiter Versuch gelingt.
- Miniaturen entstehen für jede Kachel einer Ebene, nicht nur für die sichtbaren (die sichtbaren zuerst). Liegt das Projektverzeichnis in einem Cloud-Ordner (etwa „Dokumente“ mit Synchronisierung), werden dabei alle Dokumente heruntergeladen, und `vorschau.*` wird in jedem Dokumentordner mit synchronisiert; ein Wechsel von Hell und Dunkel schreibt sie alle neu.
- Nach einem echten Absturz beim Malen zeigt die Kachel bis zu 15 Minuten „Wird gerade erzeugt …“ (ob das alte Programm noch läuft, lässt sich nicht prüfen). Danach versucht das Programm es noch einmal, was noch einmal abstürzen kann; erst dann steht „Miniatur nicht möglich“.
- Ein Dokument, dessen Miniatur einmal länger als 3 s braucht (auch nur, weil der Rechner gerade ausgelastet ist), bleibt „Miniatur zu aufwendig“, bis es eine neue Version gibt.
- „Verschieben“ zeigt keine Ziele, solange der Baum noch gelesen wird („Die Ziele werden noch gelesen …“). Jede fremde Änderung am Verzeichnis (zum Beispiel durch ein zweites Programm) schließt offene Namensfenster; ein halb getippter Name geht dann verloren.
- Tests mit Zeitannahmen: Der Test „während des Lesens ungültig machen“ nutzt eine Pause von 120 ms, der Test zum Beenden erlaubt unter 500 ms, die Klicktests legen Pausen vor Doppelklicks ein. Auf sehr langsamen Rechnern können sie flackern.
- Nicht enthalten: Suche, Sortierung, Mehrfachauswahl, Ziehen mit der Maus, Pfeiltasten (Tab und Enter gehen).

**Handliste (echtes Fenster; die Agents starten die App nie):**
- [ ] 1. Datei-Menü: „Projekte …“ mit Strg+O und „Einstellungen …“ sind da, „Öffnen …“ ist weg; „Ansicht“ hat kein „Projekte“; links steht nur noch der Feature-Baum (Modell, Zeichnung, Skizze).
- [ ] 2. Strg+O: Fenster „Projekte“ in der Mitte, etwa 55 % breit. Kacheln als Karten (Rahmen, ausgewählte mit blauem Rand), mit Miniatur, „Geändert …“ und „Version n · k Dokumente“ für deine echten Projekte. Die Uhrzeit ist Ortszeit und gleich wie im Reiter „Versionen“.
- [ ] 3. Miniaturen erscheinen nach und nach (sichtbare zuerst); bis dahin steht „Wird gerade erzeugt …“. Das Fenster bleibt bedienbar und springt nicht; beim zweiten Öffnen sind sie sofort da.
- [ ] 4. Doppelklick auf ein Projekt: Der Pfad steht in einer eigenen Zeile unter dem Titel, mit Pfeilsymbolen (nicht den Zeichen ‹ ›); Pfeil und „Projekte“ führen zurück. Darunter steht eine Zeile mit „Neues Dokument“ und „Neuer Ordner“. Doppelklick auf einen Ordner geht hinein, der Pfad wird länger (vorne mit „…“ gekürzt, wenn er nicht mehr passt).
- [ ] 5. Doppelklick auf ein Dokument öffnet es im Tab, das Fenster bleibt offen; noch einmal: nur nach vorn geholt.
- [ ] 6. Rechtsklick: Umbenennen, Verschieben (nicht bei Projekten), Löschen mit den gewohnten Rückfragen. Bei offenem Dokument: „Bitte zuerst die betroffenen Dokumente schließen.“
- [ ] 7. „Neues Projekt“, „Neuer Ordner“, „Neues Dokument“ (öffnet sofort einen Tab; die Kachel zeigt „Version 1“). Das Namensfeld nimmt gleich Eingaben an.
- [ ] 8. Die Tab-Taste läuft über die Kacheln (Fokusring außen, anders als der blaue Auswahlrand), das Fenster rollt mit; Enter öffnet.
- [ ] 9. „Projektverzeichnis …“ öffnet die Einstellungen; „Projektverzeichnis ändern …“ die Ordnerauswahl mit Schnellzugriffen; „Abbrechen“ ändert nichts. Mit offenem gespeichertem Dokument: Hinweis in Warnfarbe. Mit allen Dokumenten geschlossen: Wechsel auf einen Testordner, „Projekte“ zeigt nur dessen Projekte; zurück auf `Dokumente\PlanoraCAD`, alles ist wieder da (nichts wurde verschoben).
- [ ] 10. Hell und Dunkel umschalten, Fenster wieder öffnen: Die Miniaturen werden neu gemalt; beide Designs sind gut lesbar, auch die ausgewählte Kachel im hellen Design.
- [ ] 11. Fenstergröße 800 × 600: Kopf, Kacheln (rollen) und Fußzeile mit „Aktualisieren“ sind sichtbar. Großes Fenster: mehr Spalten, alle Kacheln gleich groß.
- [ ] 12. Netz importieren / STEP importieren / Exportieren öffnen, während „Projekte“ offen ist: „Projekte“ schließt. Strg+O bei offenem Import schließt den Import. Schreibt gerade ein Export, melden Strg+O, „Projekte …“ und „Einstellungen …“: „Bitte erst den Export beenden.“
- [ ] 13. Ein Dokumentordner im Explorer: `vorschau.png` und `vorschau.json` liegen neben `dokument.json`; beide löschen, Fenster neu öffnen: Sie werden neu angelegt.
- [ ] 14. Ein großes Dokument (zum Beispiel ein großer Netz-Körper): Die Kachel sagt „Miniatur zu aufwendig“, die Oberfläche bleibt flüssig. „Miniatur nicht möglich“ darf nur nach einem früheren Absturz beim Malen erscheinen.
- [ ] 15. Rückfrage offen (zum Beispiel Strg+S bei geändertem Dokument, „Projekte“ offen): Alle Knöpfe im Fenster „Projekte“ sind gesperrt, „Einstellungen …“ lässt sich nicht öffnen. Nach der Rückfrage geht wieder alles.

## Verlaufsleiste und Feature-Baum

### Stand und Geliefertes
- Unten im Fenster steht die **Verlaufsleiste**: die Schritte des gewählten Körpers als kleine Symbole in der Farbe des Körpers. Ein senkrechter **Marker** zeigt, bis wohin der Körper gerechnet wird. Du ziehst ihn mit der Maus, klickst auf einen Schritt oder nimmst die Knöpfe (Anfang, einen Schritt zurück, Abspielen, einen Schritt vor, Ende) und die Pfeiltasten.
- Jeder Körper hat seinen **eigenen Marker**. Zurückgesetzte Schritte sind abgeblendet, ihre Folgeschritte auch. Andere Körper bleiben davon unberührt.
- **Doppelklick** auf einen Schritt öffnet seine Einstellungen (bei einer Skizze die Skizze), auch wenn der Schritt zurückgesetzt ist. Ein geänderter Wert gilt, sobald du den Marker wieder nach vorn schiebst.
- **Sperre:** Solange ein Körper zurückgesetzt ist, kannst du nichts Neues auf ihm aufbauen (Verschieben, Verrunden, Vereinigen, Skizze auf Fläche, Extrusion seiner Skizze). Das Programm sagt es und nennt den Weg („Zum Ende“). Umbenennen, Löschen, Unterdrücken und das Ändern von Werten gehen weiter.
- Der **Feature-Baum links** ist jetzt eine Körper-Übersicht: eine Zeile je Körper mit Auge und Farbquadrat, dazu die Ordner Skizzen, Zeichnungen und Ursprung. Die alte Rückroll-Linie im Baum gibt es nicht mehr; der globale Marker bleibt nur im Menü „Bearbeiten“ („Zum Ende der Zeitleiste“) und als Knopf „Alles aktivieren“ in der Leiste.
- Neu in den Einstellungen: **Durchsichtigkeit der Panels** (0 bis 80 %). Der Baum bleibt auch bei 80 % lesbar.
- **Dateiformat 11:** Nur Dokumente mit mindestens einer Rückrollstelle werden als Format 11 gespeichert; alle anderen bleiben bei ihrer bisherigen Nummer, Byte für Byte wie vorher.

### Offene Punkte
- Zurücksetzen eines Körpers rechnet unbeteiligte Schritte mit mehreren Körpern (Verschieben, Vereinigen, Verrunden) zweimal neu, weil der Zwischenspeicher-Schlüssel alle sichtbaren Körper enthält. Bei kleinen Modellen unmerklich.
- Bei einem Fenster von 800 × 600 ist die Leiste schmal, und „n von m“ verschwindet (die Abspielknöpfe schrumpfen nicht mit).
- Nach einem Vereinigen gibt es nur eine Leiste über die ganze Kette; getrennte Leisten je Eingangskörper gibt es nicht.
- Ältere Programmversionen verlieren beim Speichern der Favoriten den Wert `panel_transparency` (unbekannte Felder in der Konfiguration gehen dort verloren, siehe oben).
- Ausgeblendete Körper (Auge) werden nie aufgeräumt: Die Merkliste wächst, bis der Tab schließt. Sie wird nicht gespeichert.
- Die Kennung einer Baumzeile hängt von ihrer Stellung ab; nach dem Umsortieren kann ein offenes Menü auf eine andere Zeile zeigen.
- `crates/planora-app/src/browser.rs` hat etwa 1170 Zeilen und sollte in Baum, Zeilen und Menüs aufgeteilt werden.
- Im durchsichtigen Baum haben gesperrte Zeilen in schreibgeschützten Versionen weniger Kontrast (0,75 Deckung), die Prüfung deckt das nicht ab.
- Dokumente mit Rückrollstellen (Format 11) kann ein älteres Programm nicht öffnen; es meldet „zu neu“.
- **Export:** STEP, Netz und PDF schreiben den Stand, wie er gerade ist: zurückgesetzte Körper fehlen, und mit dem Auge ausgeblendete Körper sind trotzdem dabei (das Auge gilt nur für die Ansicht). Ein Hinweis im Ausgabefenster erscheint nicht. Wer mitten in der Wiedergabe anhält und exportiert, bekommt also nicht das fertige Teil.
- **Stücklisten-Folgeblätter** werden bei jedem Zurücksetzen oder Vorschieben gelöscht und mit neuen Kennungen wieder angelegt (das war schon vorher so, fällt mit dem Abspielen aber öfter auf). Eigene Änderungen auf Folgeblättern gehen dabei verloren.
- **Abspielen über ein Vereinigen** passt die Kamera neu an, sobald sich die Zahl der Körper ändert.
- **Körperspitzen** werden je Schritt gezählt, nicht je entstandenem Körper: Hat ein Schritt mehrere Ergebnis-Körper und wird nur einer davon weiterverwendet, behandeln Leiste und Sperre beide als einen Körper. Im Baum geht nichts verloren.
- Nach einem zurückgesetzten Vereinigen sind beide Eingangszeilen im Baum zusammen hervorgehoben.
- Bei sehr schmalen Fenstern kann der Statustext der Leiste so viel Platz nehmen, dass nur ein Schritt sichtbar bleibt.
- In Dokumenten mit mehreren Körpern kostet das Bewegen des Markers (und damit das Abspielen) Rechenzeit, weil die Schritte der anderen Körper mit neu gerechnet werden. Bei vielen Verrundungen kann das ruckeln; im Fenstertest mit einem solchen Dokument prüfen.
- Die Prüfung vieler Rückrollstellen beim Laden kostet insgesamt noch quadratische Zeit, weil `Document::chain()` je Aufruf linear ist (eine Berechnung der Ketten in einem Durchlauf würde das beheben). Das spielt nur bei absichtlich gebauten Dateien eine Rolle.
- Zurückgestellt aus den Prüfungen: Der Marker hat 12 px Trefferfläche und überdeckt dabei Nachbarschritte etwas; Dialoge „Erzeugen“ und „Bearbeiten“ sperren die Leiste nicht (wie das Befehlsfeld).

### Handliste (echtes Fenster)
- [ ] 1. Quader und Zylinder anlegen, verschieben, vereinigen: Die Leiste zeigt eine Kette in den Farben der Körper, das Farbquadrat im Baum passt dazu.
- [ ] 2. Marker ziehen und mit den Pfeiltasten bewegen; Abspielen läuft im Takt und hält am Ende.
- [ ] 3. Doppelklick auf einen zurückgesetzten Schritt: Die Einstellungen öffnen sich, der Marker bleibt stehen.
- [ ] 4. Sperre: Auf einem zurückgesetzten Körper „Vereinigen“ wählen: deutscher Hinweis, nichts angelegt.
- [ ] 5. Durchsichtigkeit auf 0, 40 und 80 %, hell und dunkel: Der Baum bleibt lesbar.
- [ ] 6. Auge in der Körperzeile: Der Körper verschwindet aus der Ansicht, nach dem Neustart ist er wieder da.

## Teilprojekt: Tastenkürzel

Entwurf: `docs/superpowers/specs/2026-10-24-tastenkuerzel-design.md` (mit Nachtrag „Tatsächlich umgesetzt“). Plan mit „Abweichungen“: `docs/superpowers/plans/2026-10-24-tastenkuerzel.md`. Zweig `feature/tastenkuerzel`.

### Stand und Geliefertes
- Alle Kürzel stehen in **einer Tabelle** (`planora-app/src/keymap/defaults.rs`, Makro `action_table!`): Id, deutscher Name, Bereich (Überall, Modell, Skizze, Zeichnung), Voreinstellung. Menüs, Knöpfe und Tooltips zeigen die aktuelle Taste aus der Belegung.
- Ab Werk wie in Fusion: E Extrudieren, F Verrunden, V Sichtbarkeit, Entf Löschen, Leertaste letzten Befehl wiederholen, S Übersicht; in der Skizze L, R, C, P, X; in der Zeichnung D (Maß) und B (Position). Dazu die bisherigen Dateikürzel (Strg+N, O, S, W, Umschalt+E, Z, Y).
- Maus ab Werk wie Fusion: mittlere Taste schwenkt, Umschalt+mittlere dreht, Rad zoomt (auch Strg+Rad für das Touchpad); die linke Taste dreht weiter, Umschalt+links schwenkt.
- Fenster Einstellungen, Abschnitt „Tastenkürzel“: Tabelle mit Suche, Ändern durch Tastendruck oder Mausgeste, Zurücksetzen je Zeile und für alles, Hinweis bei Doppelbelegung mit „Tauschen“. Gespeichert werden nur Abweichungen in der Konfigurationsdatei.
- Übersicht „Tastenkürzel“ (Ansicht → Tastenkürzel … oder S) mit Suche nach Name und Taste.
- Umschalttasten zählen genau (Strg+Umschalt+N und Strg+Alt+Z feuern nicht mehr).

### Regel für künftige Funktionen
Hat Fusion für eine neue Funktion ein Kürzel, gilt: **Taste eintragen, sobald die Funktion gebaut wird.** Bis dahin steht sie nur in der folgenden Liste.

### Fusion-Kürzel ohne Funktion (nicht belegt, Grund: Funktion fehlt)
- Modell: M Verschieben (kein Verschieben-Werkzeug, nur Positionsfelder), H Bohrung, I Messen, Q Drücken/Ziehen, A Erscheinungsbild, Strg+B Alles neu berechnen, 1 / 2 / 3 Auswahlarten.
- Überall: Strg+C / Strg+X / Strg+V (Kopieren, Ausschneiden, Einfügen), Strg+Umschalt+S (Wiederherstellungs-Sicherung).
- Skizze: O Versatz, T Stutzen, D Bemaßung.
- Zeichnung: C Mittelpunktmarke, M Verschieben, P Projizierte Ansicht, T Text.
- Maus: „Um Punkt drehen“ (Umschalt+Klick, dann mittlere Taste halten) ist nicht gebaut.

### Offene Punkte und bekannte Eigenheiten
- **Strg oder Alt + linke/mittlere Taste ziehen** dreht oder schwenkt nicht mehr (früher drehte und schwenkte jede Strg-/Alt-Stellung mit). Bewusst so, weil die Umschalttasten genau zählen; die Belegung „links dreht“ gilt nur ohne Strg/Alt.
- **Deutsche Tastatur:** Tasten, die egui nicht kennt, erscheinen als die Taste auf der Hardware (Ü steht als „Eckige Klammer auf“). Sie funktionieren, der Name ist nur irreführend.
- **V** blendet den Körper aus und leert die Auswahl (wie ein Klick aufs Auge). Ein zweites V tut darum nichts; der Körper muss erst wieder gewählt werden.
- Wird ein Fenster **geschlossen, während eine erfasste Taste noch gehalten wird**, kann die Wiederholung der Taste einmal auslösen.
- **Kontextmenüs und Skizzenzüge** während des Erfassens einer Mausgeste sind nicht besonders behandelt (ein Fenstertest fehlt).
- **Esc bei gestapelten Fenstern** ist nicht besonders geregelt; die Übersicht schließt dagegen immer mit Esc.
- **Einstellungen** öffnen oben, nicht beim Abschnitt „Tastenkürzel“ (auch nicht aus der Übersicht).
- **Würfel (Ansichtswürfel):** Das Drehen am Würfel folgt der Tabelle. Entfernt man die Belegung „links dreht“, dreht der Würfel in der Skizze nicht mehr mit links; Umschalt+mittlere Taste dreht ihn weiter.
- **Letztes Skizzenwerkzeug:** Beim Spiegeln oder Muster kann ein falsches Werkzeug gemerkt werden (die Kurve entsteht nicht durch das gewählte Werkzeug).
- **Doppelbelegung:** Der Hinweis, dass eine Taste schon einer anderen Aktion gehört, kommt nur in den Einstellungen. Beim Laden einer von Hand bearbeiteten Datei gilt bei Doppelbelegung die erste Aktion der Tabelle.
- Einträge in der Konfiguration, die nur eine neuere Version kennt, gehen beim Ändern in einer älteren Version verloren (Tester wechseln nicht zurück; angenommen).

## Installer

- **Keine Signatur (SmartScreen):** Der Installer ist nicht signiert, Windows warnt beim ersten Start. Bleibt liegen: Ein Zertifikat kostet Geld und lohnt sich erst mit mehr Nutzern; die README erklärt den Weg über „Weitere Informationen“.
- **UAC-Abfrage blockiert das Programmfenster kurz:** Bei „Für alle Benutzer“ fragt Windows beim Update nach Administratorrechten; währenddessen ist das Programmfenster kurz nicht bedienbar. Bleibt liegen: Das ist Windows-Verhalten und dauert nur Sekunden.
- **Inno startet die neue Version bei „für alle“ über explorer.exe:** Das Programm läuft danach mit den normalen Rechten des Benutzers, nicht mit Administratorrechten (gewollt). Bleibt liegen: Es gibt dadurch keinen Nachteil, nur einen anderen Startweg als bei „nur für mich“.
- **Neustart bei „für alle Benutzer“ mit Standard-Benutzerkonto ungeprüft:** Gibt man das Administratorkennwort eines anderen Kontos ein, ist unklar, ob der Neustart über explorer.exe PlanoraCAD als angemeldeter Benutzer startet. Nur mit einem zweiten Konto prüfbar.
- **Dateitausch im kurzen Fenster:** Zwischen der Freigabe der Sperre nach ShellExecuteExW und dem erneuten Öffnen durch Inno könnte ein Schadprogramm desselben Benutzers die Datei tauschen. Bleibt liegen: außerhalb des Bedrohungsmodells.
- **Stiller Installer scheitert nach dem Beenden:** Scheitert der Installer, nachdem sich die App beendet hat, startet nichts neu. Die einzige Spur steht im Protokoll `planoracad-update.log`.
- **Noch nie auf GitHub gelaufen:** Der Trockenlauf von `release.yml` mit dem Installer-Schritt (Inno Setup über Chocolatey, Version 6.7.3) steht aus.

## Update-Funktion

Stand: 2026-10-25. Aufgaben 1 bis 7 sind gebaut (Prüfung, Fenster, Einstellungen, Austausch mit Rücknahme, Veröffentlichungsablauf, Aufräumen). Das Ausrollen einer ersten Version ist noch nicht passiert.

### Vor dem ersten Release (nur auf ausdrückliches Wort von Dennis)
- **Tag setzen und pushen:** `v0.2.0-dev.1` setzt und pusht nur Dennis (nicht das Skript, nicht ein Hilfsprogramm). Vorher die volle Prüfung `ci.yml` von Hand auf genau diesem Commit starten; `tools/release.ps1` prüft das.
- **Trockenlauf von `release.yml`** auf GitHub (von Hand starten, ohne Tag) ist noch nie gelaufen. Das Release anzulegen ist nur lokal geprüft: Skripte per Probelauf, YAML mit einem Parser; die GitHub-Schritte selbst (Rust-Cache, `gh release create`) wurden nicht ausgeführt.
- **Erste echte Aktualisierung** von einer Testversion aus (mit der Testquelle `update-test-source`, danach gegen ein echtes Release) steht aus. Die echte Netzabfrage wurde noch nie ausgeführt (auf diesem Rechner fängt NordVPN die TLS-Verbindung ab; bei Fehlern erst mit curl/OpenSSL klären).
- Die Markenrecherche für „PlanoraCAD“ (siehe oben) gilt auch für ein öffentliches Release.

### Grenzen
- Updates nur unter Windows (x64). Linux und macOS sind nicht vorgesehen.
- Die Programmdatei ist nicht signiert; Windows kann beim ersten Start des heruntergeladenen Programms warnen (SmartScreen).
- Es gibt nur den Kanal Dev. Beta und Stabil sind in der Versionsprüfung vorbereitet, aber nicht wählbar.
- Quelle ist ausschließlich die Release-Liste von GitHub; bei Netzfehlern oder Ablauf der GitHub-Abfragegrenze bleibt die Suche still.
- `THIRD-PARTY-LICENSES.txt` enthält alle Bibliotheken, die Cargo für die Windows-Version auflöst. Das sind etwas mehr als die tatsächlich gebauten (Funktionen, die nur von Testcode eingeschaltet werden); das ist gewollt, damit nichts fehlt. 17 Pakete bringen keine eigene Lizenzdatei mit; für sie setzt das Skript den Standardtext der angegebenen Lizenz ein (Texte in `tools/license-texts`). Bei neuen Bibliotheken mit anderer Lizenz bricht das Skript ab, bis ein Text ergänzt ist.
- Das Versionsfeld in `Cargo.toml` und der Abschnitt in `CHANGELOG.md` müssen zum Tag passen; sonst bricht `release.yml` ab (gewollt).

### Zurückgestellte Kleinigkeiten
Alles hier wurde bei den Prüfungen der Aufgaben 1 bis 5 gefunden, ist klein und hat die Aufgabe nicht blockiert. Nichts davon ändert, was ein Nutzer im Normalfall erlebt. Bei jedem Punkt steht, warum er liegen bleibt.

**Prüfung der Version und der Veröffentlichungen (Aufgabe 1)**
- Eine Veröffentlichung wird ganz übergangen, wenn ein fremdes Anhangsfeld (zum Beispiel die Größe) fehlt. Bleibt liegen: GitHub liefert die Felder immer; es fiele höchstens eine Version aus.
- Die Kennzeichnung „gekürzt“ meldet sich auch, wenn der Platz genau ausgefüllt ist. Bleibt liegen: nur ein überflüssiger Hinweis „und weitere Änderungen“.
- Die Änderungsnotizen können mitten in einer Zeile abgeschnitten werden und kennen einige seltene Schreibweisen nicht (BOM, Tabulator nach dem Zeichen, einzelnes Zeilenende, `***x***`). Bleibt liegen: nur das Aussehen von Randfällen.
- Fehlende Tests: Beta-schließt-Dev-ein mit anderer Versionsnummer, genaue Fehlerarten bei kaputtem JSON, „erste doppelte Version gewinnt“, „Vorabversion-Kennzeichen wird ignoriert“, ein Test für `Version::from_cargo` in der App. Bleibt liegen: das Verhalten ist durch andere Tests abgedeckt, es fehlt nur die Einzelabsicherung.
- Der Entwurf verspricht eine Meldung bei einer kaputten Veröffentlichung, der Code überspringt sie still und nimmt die nächstältere. Bleibt liegen: so gewollt; der Entwurf sollte bei Gelegenheit angepasst werden.

**Einstellungen (Aufgabe 2)**
- Magische Zahl `menu_bottom = 50.0` im Test `settings_updates.rs` ohne Kommentar. Bleibt liegen: nur im Test.
- Die Warnung „Einstellungen nicht gespeichert“ bleibt stehen, wenn ein anderer Weg die Datei später doch geschrieben hat. Bleibt liegen: nur ein überholter Hinweis in einem Fehlerfall.
- Der Block für das vorgemerkte Update steht in `workspace.rs` mehrfach (Hilfsstruktur wäre schöner). Bleibt liegen: reine Aufräumarbeit ohne Wirkung.

**Prüfung im Netz (Aufgabe 3)**
- Die Größenbegrenzung der Antwort (2 MB) hat keinen eigenen Test. Bleibt liegen: das Lesen müsste dafür in eine testbare Hilfsfunktion; der Wert selbst ist einfach.
- Gesamtzeit 30 Sekunden (je Abschnitt 10 s), der Entwurf nennt 10 s. Bleibt liegen: die Prüfung läuft im Hintergrund und stört nicht.
- Die Abfrage der Release-Liste folgt Weiterleitungen ohne eigene Grenze. Das ist eine Sicherheitsfrage mit geringem Risiko, weil nur HTTPS erlaubt ist und die Antwort ohnehin streng geprüft wird. Bleibt liegen: die Download-Seite (Aufgabe 5) ist bereits streng begrenzt; die Liste sollte bei Gelegenheit gleich behandelt werden.
- Das Ergebnis einer von Hand gestarteten Prüfung geht verloren, wenn die Einstellungen währenddessen geschlossen und wieder geöffnet werden. Bleibt liegen: selten und folgenlos, man startet die Prüfung noch einmal.
- Eine Antwort 403 mit „Retry-After“ wird als allgemeiner HTTP-Fehler 403 gemeldet. Bleibt liegen: die Suche bleibt in beiden Fällen still.
- Das Feld `handle` in `UpdateChecker` wird nur gehalten, nie gelesen. Bleibt liegen: harmlos, hält den Hintergrundfaden fest.

**Fenster „Neue Version“ (Aufgabe 4)**
- „Überspringen“ ignoriert, wenn das Schreiben der Einstellungen scheitert (`update_dialog.rs`). Bleibt liegen: sehr seltener Fehlerfall; die Version käme beim nächsten Start noch einmal.
- Die Meldung kann mitten in einer Skizze oder beim Ziehen aufgehen. Bleibt liegen: man schließt sie mit Esc, nichts geht verloren.
- Testlücken (Dateifenster offen, Export läuft, sehr lange Adressen) und etwas Rauschen im Test (`let _ = &mut h;`). Bleibt liegen: nur Tests.
- Die Notizen werden in jedem Bild neu zerlegt statt einmal beim Öffnen. Bleibt liegen: kostet bei der Obergrenze von 20 000 Zeichen kaum etwas.
- Das Fenster „Über“ kann neben der Meldung offen bleiben (wie bei der Tastenkürzel-Übersicht). Bleibt liegen: nur ein Schönheitsfehler.
- „Später“ gilt für 24 Stunden für jede Version, die Statuszeile zeigt eine neuere trotzdem. Eine offene Meldung behält die alten Notizen, wenn eine neue Prüfung eine andere Version liefert. Bleibt liegen: beides sind Randfälle ohne Folgen.

**Aus der Gesamtprüfung (zurückgestellt)**
- Die Rust-Version und `cargo-deny` sind in den Abläufen nicht auf einen festen Stand gesetzt, und `release.yml` benutzt beim Bau der Veröffentlichung den Rust-Cache. Bleibt liegen: ein fester Stand wäre sauberer und unabhängiger von Zwischenständen; bisher ist kein Lauf auf GitHub gemacht, ein Fehler fiele dort sofort auf.
- Die Abfrage der Release-Liste folgt Weiterleitungen ohne die strengen Regeln des Downloads (siehe auch Aufgabe 3). Bleibt liegen: nur HTTPS, die Antwort wird streng geprüft, und heruntergeladen wird nur über die strengen Regeln.
- Der Austausch sind zwei Umbenennungen nacheinander; fällt genau dazwischen der Strom aus, fehlt die Programmdatei und nur `.old` liegt da. Bleibt liegen: das Fenster ist winzig, und die README erklärt, wie man `.old` von Hand zurückbenennt.

**Herunterladen und Austauschen (Aufgabe 5)**
- Wird „Abbrechen“ genau dann gedrückt, wenn der Download fertig wird, bleibt die geprüfte `.new`-Datei liegen. Dasselbe gilt, wenn das Fenster mitten im Download geschlossen wird. Sie wird beim nächsten Versuch gelöscht. Bleibt liegen: braucht Platz (etwa 100 MB), aber nur bis zum nächsten Update.
- Ein Absturz oder Stromausfall genau zwischen den zwei Umbenennungen beim Austausch lässt `planoracad.exe` fehlen; es liegen dann `planoracad.exe.old` und `planoracad.exe.new` da. Ebenso, wenn der alte Prozess beendet wird, während die neue, kaputte Version wartet. Bleibt liegen: das Fenster ist winzig; die Anleitung steht im README („`.old` zurückbenennen“).
- Mit `--nach-update` und einem veralteten Eintrag wird trotzdem eine Rückmeldung geschrieben. Bleibt liegen: nur wenn die Versionsnummer im Programm nicht zum Tag passt; das prüft `release.yml`.
- Die Schreibprobe im Programmordner (`exe_dir_writable`) kann bei kurz gesperrter Datei fälschlich „geschützter Ordner“ melden und lässt dann eine Probedatei liegen. Bleibt liegen: sehr selten, Folge ist nur ein Hinweis mit Weg zur Download-Seite.
- Die Übergabedatei für die Änderungsnotizen wird bei einem Fehler nicht gelöscht, sondern erst beim nächsten normalen Start. Bleibt liegen: sie ist klein und harmlos.
- Eine liegengebliebene `.new`-Datei aus einer unterbrochenen Sitzung wird erst beim nächsten Update gelöscht (siehe oben). Bleibt liegen: wie oben.
- `app.rs` ist mit rund 1900 Zeilen sehr lang; der Neustart-Ablauf gehörte in `update_install.rs`. Bleibt liegen: Umbau ohne Nutzen für den Anwender, hohes Risiko ohne ausdrücklichen Auftrag.
- Während „Update bereit, wartet auf …“ gibt es dort weder „Abbrechen“ noch „Jetzt neu starten“, und Schließen mit ungespeicherten Tabs wird dabei zu „aktualisieren und neu starten“. Auch ein Angebot zur Wiederherstellung, das mitten in einer Rückfrage eintrifft, sollte warten. Bleibt liegen: nur in Randlagen; Rückfragen zu ungespeicherter Arbeit bleiben erhalten.
- Nur mit der Testquelle (`update-test-source`, nie im Release): Eine Weiterleitung von GitHub auf `http://127.0.0.1` wird akzeptiert. Bleibt liegen: der Schalter ist standardmäßig aus.

**Aus der Prüfung der Aufgabe 5b**
- Sehr lange Projektpfade können das Einstellungsfenster bei 800 × 600 verbreitern. Bleibt liegen: Vorgabe ist mindestens 1920 × 1080.

**Zum Veröffentlichen (Aufgabe 6)**
- Erledigt in Aufgabe 7: Aktionen auf festen Commit gesetzt, Trockenlauf ohne Schreibrecht (zwei Aufträge), robustere Versionssuche, `release.ps1` prüft Tag und `Cargo.lock`, Abschnitte im Changelog dürfen nicht leer sein.
- Es bleibt: Die GitHub-Schritte sind nie gelaufen (siehe oben, Trockenlauf).

**Beobachtungsliste `cargo deny`**
- Die Hinweise zu doppelten Paketen `core-foundation`, `foreign-types` und `foreign-types-shared` stehen auf der Beobachtungsliste; die neuere Fassung von `core-foundation` kommt über `native-tls` (nur der macOS-Zweig, wird unter Windows nicht gebaut), die ältere über das Fenstersystem (`winit`). Bleibt liegen: nur Hinweise, keine Fehler; Lizenzen und Sicherheitsmeldungen sind in Ordnung. Bei jedem Update erneut ansehen (zusammen mit der zbus-Familie und `ttf-parser`).

## Rückmeldungen

Stand nach Etappe 6 (Plan `docs/superpowers/plans/2026-10-26-rueckmeldungen.md`).

**Was geht:** Rückmeldung aus dem Programm (Hilfe → Rückmeldung …, Strg+Umschalt+R) mit Vorschau und Häkchen je Angabe, Bild vom Fenster, Weg über GitHub und Weg ohne Konto über das Webformular auf dennis-guse.de, Absturzbericht mit Rückfrage beim nächsten Start (ohne Benutzername, Projekt als `.pcad.zip`), auflösbarer Fehlerablauf mit `tools/symbolize.ps1` und der `.pdb` am Release, „Ausführliches Protokoll“ unter Einstellungen → Updates, Projekttafel mit Auto-Aufnahme: https://github.com/users/KaelanTesseract/projects/1.

**Was nicht geht:**
- Abstürze ohne Rust-Meldung (hart beendet, Stromausfall, Absturz im Treiber) schreiben keinen Bericht. Das Programm weist beim nächsten Start nur darauf hin.
- Die Mail an info@dennis-guse.de ist nur der Notweg; das Webformular ist der Weg ohne Konto. Es nimmt höchstens 10 Dateien (je 25 MB, zusammen 30 MB; PNG, JPG, TXT, LOG, ZIP).
- Ein Link kann keine Dateien mitnehmen. Bild, Bericht und Projekt hängt der Anwender im Browser selbst an.
- Das Projekt-ZIP für die Rückmeldung wird im Vordergrund geschrieben; bei großen Projekten friert das Fenster kurz ein.
- Der Absturzbericht enthält Protokollzeilen und Schritte auch ohne das Häkchen „Protokoll“ (in der Vorschau sichtbar).
- Windows-Abmelden oder Herunterfahren bei laufender App könnte beim nächsten Start „unerwartet beendet“ zeigen (nicht geprüft).
- Die GitHub-Schritte von `release.yml` sind noch nie auf GitHub gelaufen (Trockenlauf steht aus); die `.pdb` (etwa 91 MB) ist dort noch nicht als Release-Datei geprüft.

**Zurückgestellt (je ein Satz):**
- „Adresse anzeigen“ und „Text kopieren“ zeigen die GitHub-Adresse, auch wenn später das Webformular gewählt wird.
- „Letzten Befehl wiederholen“ trägt zwei Schritte in den Ablauf ein; das ist als Zusammenhang gewollt.
- Das Bild vom Fenster hat keine Vorschau als Bild, nur eine Zeile mit der Größe.
- Laufen zwei Instanzen gleichzeitig, kann die Rotation des Protokolls eine alte Datei früher verlieren.
- Im Absturz-Hook ist die Sperre gegen doppeltes Auslösen nicht testbar, das Dokument fehlt im Bericht, wenn das Protokoll gesperrt ist, der Pfad wird in jedem Bild einmal kopiert, und die Hook-Tests räumen nicht über einen Drop-Wächter auf.
- Der Absturzbericht hat keine Tests für einen leeren Benutzernamen, Nicht-ASCII-Namen und für ein fehlgeschlagenes Verschieben der Begleitdatei; die ZIP-Ausgabe liest die ganze Datei in den Speicher.
- Bei `tools/symbolize.ps1` ist die Begrenzung von `SizeOfImage` nicht getestet, die geladene `.pdb` bleibt im Temp-Ordner, und `release_meta.rs` liest nur die oberste Ebene von `src`.
- Im Webformular-Knopf stehen noch wörtliche Zeilenumbrüche in Zeichenketten (`feedback.rs`), es fehlt ein Test für die letzte Rettung ohne Version und System und ein Grenztest nur für das Formular.
- Die Schwellen für schmale Fenster liegen seit dem Menü „Hilfe“ 39 Pixel höher (von Dennis angenommen).
- Die Feldfarben im hellen Aussehen sind nicht einzeln geprüft worden, sondern nur über Tests und die Gesamtprüfung von Etappe 3.

## Sprachen

Teilprojekt Sprachen (Deutsch und Englisch, weitere Sprachen über Dateien), abgeschlossen und am 2026-10-30 nach master gemergt (CI grün, 4429 Tests). Zurückgestellt:
- Datumsangaben bleiben auch im Englischen in der deutschen Form TT.MM.JJJJ (z. B. „23.10.2026, 18:42“ in Projekte-Fenster und Versionen).
- Zoll und andere Einheiten: eigenes Teilprojekt, siehe unten „Später: Teilprojekt Zoll“.
- Schrift von rechts nach links (Arabisch, Hebräisch) und Schriften, die die eingebauten Schriftarten nicht abdecken (z. B. Chinesisch, Japanisch), gehen nicht; die Anleitung `docs/uebersetzen.md` sagt das.
- Mitgeliefert werden nur Deutsch und Englisch; der Installer legt den leeren Ordner `languages` neben der exe an.
- (Behoben am 2026-10-30:) Neue Schnittansichten wurden automatisch über das Schriftfeld gelegt; jetzt weichen sie auf die Gegenseite aus oder rücken enger (siehe `docs/funktionen.md`, „Schnitt anlegen“).

## Später: Teilprojekt Zoll

- Maße in Zoll statt Millimeter (für die USA). Beim Entwurf der Sprachen am 2026-10-27 bewusst ausgeklammert; eigenes Teilprojekt (Dennis).
