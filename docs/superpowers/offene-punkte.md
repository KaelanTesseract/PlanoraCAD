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
- Das Ein-/Ausblenden von Projekten und Versionen sowie der gewählte Reiter rechts (Eigenschaften oder Versionen) wird nicht gespeichert; bei jedem Start sind beide Felder sichtbar (seit Etappe B feste schwebende Felder statt Dock).
- Schreibgeschützte Tabs sperren nur die Oberfläche (Browser, Eigenschaften, Menüs „Erzeugen“/„Bearbeiten“); nicht jede Zustandsmethode in `AppState` prüft den Schreibschutz.
- Die Wahl eines Zweigs im Versionen-Panel öffnet seine Spitze als Ansicht; der Arbeitsstand des bearbeitbaren Tabs bleibt unberührt.

**Tests**
- Schwache Plantests, bewusst nicht geändert: `the_tree_is_not_reread_every_frame` (Teil der Aussage ist immer wahr). Es fehlen strengere Tests: ein auf der Platte neu angelegtes Projekt erscheint erst nach „Aktualisieren“; ein zwischengespeicherter Lesefehler; Umbenennen/Verschieben/Löschen bei Ansichten und bei anderer Groß-/Kleinschreibung; die „Ansicht“-Schalter.
- `PlanoraApp::logic` selbst (die drei Zeilen Verdrahtung des Schließablaufs) ist nicht ohne Grafikkarte testbar; getestet ist `handle_close_request`.

**Aktualisierung früherer Abschnitte**
- „Kein Speichern und Laden“ in den Abschnitten zu den Teilprojekten 1–5 gilt nicht mehr. „Kein Rückgängig“ gilt weiter.

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
- Werkzeug: Ein Klick auf den Umriss eines Zylinders misst in linearen Maßen bis zur Achse (Hinweis erscheint). Die Richtung einer Kette folgt automatisch dem größeren Wert; bei fast diagonalen Punkten kann sie kippen. Esc setzt eine Kette, sonst bricht es ab; Doppelklick wirkt nur bei Ketten. Scheitert eine Kette in der Mitte, bleiben die schon gesetzten Maße. Ein Maß wird nahe Ecken vor den Ansichtskanten getroffen; der Abstand 0 zählt als positiv; der kleinste Abstand beim Ziehen ist 1 mm (die Seite wechselt man derzeit nicht; ein Eingabefeld für den Abstand folgt). Zieht man die Maßlinie eines Glieds einer Kette oder von Bezugsmaßen, wandert die ganze Gruppe um dasselbe Stück mit (Text und Führungslinien ziehen weiter einzeln). Das Ziehen schreibt je Bild eine Änderung, das Rückgängigmachen einer Kette sind n Schritte (es gibt noch kein Rückgängig).
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
  4. Rückgängig/Wiederholen für Maße, auch als ein Schritt je Ziehen und je Kette.
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
- Höhe nach Inhalt (Abschluss-Nachbesserung): Der Rollbereich ist ein eigener unter der Kopfzeile (nicht der des egui-Fensters, der jeden Dialog auf 420 px streckte). Die Kopfzeile hat eine feste Zeilenhöhe, sonst misst egui sie im ersten Durchgang niedriger und der Dialog stünde ein Bild lang versetzt. Ein wieder geöffneter Dialog verwirft sein erstes Bild, damit er mit der neuen Höhe mittig steht. Der Titel ist nicht markierbar, damit Ziehen am Titel den Dialog verschiebt.
- Die Statuszeile ist ein Tab-Halt für die Bedienhilfe; mit Fokus zeigt sie den Fokusrahmen, und Esc, Entf und Eingabe wirken weiter auf Skizze und Ansicht (`viewport::focus_is_not_typing`).
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
