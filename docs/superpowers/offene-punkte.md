# Offene Punkte nach Teilprojekt 1–3

Stand: 2026-10-09. Ergebnis der Abschlussprüfungen; bewusst nicht in Teilprojekt 1–3 behoben.

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

## Vor Teilprojekt 4 (Skizzen)
- `ProfileLoop::polygon` panict bei < 3 Punkten (öffentliche API) → Result.
- `ProfileLoop::sample`/`signed_area` sind öffentlich und ungeschützt (riesige Winkel).
- Sich überschneidende oder außerhalb liegende Löcher werden nicht erkannt.
- `Frame`-Eingaben (NaN, nicht orthonormal) werden in `extrude`/`revolve` nicht geprüft.
- UI soll Eigenschaften über eine Kernel-Fassade lesen statt über `solid.faces[..]`.

## Aufräumen / Qualität
- `rust-version = "1.88"` ist für die GUI-Crates ungenau (egui 0.36 braucht 1.95).
- `eframe` mit Standard-Features baut auch den `glow`-Renderer mit.
- CI baut `cargo-deny` bei jedem Lauf neu.
- Tests: Pan-/Orbit-Vorzeichen, Auswahl vor Hover, Vertex-Größen, `UnsupportedSegment`, Torus bei Uhrzeigersinn-Kette.
- Auswahl: CPU-Raycast läuft bei jeder Mausbewegung über alle Dreiecke (später Hüllkörper/Raumindex).
- Hover-Änderung baut die ganze Szene neu und lädt sie hoch (später nur Farben aktualisieren).
- Browser: lange Namen werden abgeschnitten; Klick im Browser setzt keine Flächenauswahl.
