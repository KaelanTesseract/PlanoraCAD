# Offene Punkte nach Teilprojekt 1+2

Stand: 2026-10-09. Ergebnis der Abschlussprüfung; bewusst nicht in Teilprojekt 1+2 behoben.

## Entscheidung des Projekteigners nötig
- **`cargo deny check` ist rot** (Spec §9.5, CI-Schritt): 
  - Lizenz: `epaint_default_fonts 0.36.2` = `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0` (die in `egui` mitgelieferten Schriften). Wege: schmale Ausnahme (`[[licenses.exceptions]]`) nur für dieses Crate, oder `egui`-Standardschriften abschalten und eine geprüfte Schrift mitliefern.
  - Hinweise „unmaintained“: `paste 1.0.15` (RUSTSEC-2024-0436, über `egui_dock`) und `ttf-parser 0.25.1` (RUSTSEC-2026-0192, über `winit`/`ab_glyph`, Linux-Pfad). Keine Sicherheitslücken; ggf. mit Begründung in `ignore` aufnehmen.
- **Markenrecherche** für „PlanoraCAD“ (DPMA, EUIPO, WIPO, Klasse 9) vor einer Veröffentlichung.
- **Linux:** App-Start dort noch nicht geprüft (CI baut und testet nur).

## Vor Teilprojekt 3 (Boolesche Operationen)
- Arena-getaggte IDs (IDs verschiedener Körper sind heute austauschbar).
- Validator: Kantenendpunkte gegen Eckpositionen prüfen (`curve.point(t0/t1)`).
- Schwellen aus `Tolerance` ableiten (z. B. feste 1e-12 bei Flächenprüfung).
- Exakte Prädikate (`orient2d`) für Orientierungsentscheidungen nutzen.
- Beliebig berandete gekrümmte Flächen (heute nur rechteckige UV-Ausschnitte).

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
