# Torusbänder ohne Naht beim STEP-Import – Entwurf

Stand: 2026-10-31. Dennis hat die Einzelheiten an Claude übergeben („entscheide du“); alle
Festlegungen unten sind Claudes Entscheidung.

## Warum

Der STEP-Leser fügt Zylinder-, Kegel- und Kugelflächen ohne Naht eine Naht hinzu (P9,
`crates/planora-step/src/read/seam.rs`). Torusflächen ohne Naht lässt er mit „Fläche vom Typ
Torus ohne Naht wird noch nicht unterstützt.“ aus. Programme, die Flächen ohne Naht schreiben,
tun das aber gerade bei Rundungen an Kreiskanten (Welle mit gerundeter Kante, Bohrung mit
gerundetem Rand), bei Rohrbögen und bei Ringen. Solche Körper fehlen heute nach dem Import.

## Ziel

Diese drei Formen kommen an, mit richtigem Volumen und richtiger Seite:

1. **Band um die Achse** (Rundung zwischen zwei Kreisen): zwei Ränder, die sich je einmal um
   die Achse des Torus winden (Breitenkreise, `v` fest), gegenläufig. Naht: ein Kreis um das
   Rohr (Meridian) beim Winkel `u_s`, vom einen Rand zum anderen, über das Band.
2. **Rohrbogen**: zwei Ränder, die sich je einmal um das Rohr winden (Rohrkreise, `u` fest),
   gegenläufig. Naht: ein Kreisbogen um die Achse (Breitenkreis) beim Winkel `v_s`, vom einen
   Rohrkreis zum anderen, über den Bogen.
3. **Voller Ring ohne Rand**: zwei Nähte durch eine neue Ecke bei `(u, v) = (0, 0)`, in der Form,
   die der Leser schon kennt (Hohlraum „Ringhöhle“ in `tests/data/void_box.step`):
   `[Breitenkreis, Meridian, Breitenkreis rückwärts, Meridian rückwärts]`.

In allen drei Formen dürfen weitere Ränder als Löcher in der Fläche liegen (etwa eine Bohrung
quer durch das Band). Die Naht hält Abstand zu ihnen wie heute beim Zylinder.

## Nicht-Ziele

- Torus mit nur Löchern als Rand (voller Ring mit einer Querbohrung): unverändert wie bisher.
  Ränder, die sich in beiden Richtungen winden, und mehr als zwei windende Ränder: Meldung
  „schließt sich nicht“, der Körper wird ausgelassen.
- Spindeltorus (kleiner Radius ≥ großer): bleibt wie bisher ausgelassen.
- Änderungen am Kernel. Der Kernel kann Torusflächen mit Naht in `u` oder `v` schon darstellen
  (Rundungen, Drehkörper, Hohlraum „Ringhöhle“).
- Freiformflächen, die in zwei Richtungen geschlossen sind.

## Anti-Ziele

- Ein Körper, der mit falschem Volumen oder auf der falschen Seite ankommt, ist schlimmer als
  ein ausgelassener. Passt etwas nicht, bleibt der Körper mit einer Meldung draußen.
- Bisher richtig gelesene Dateien dürfen sich nicht ändern (alle vorhandenen Tests bleiben grün).

## Wie (Claudes Festlegungen)

- **Seite des Bands aus dem Umlaufsinn, nicht aus der Lage.** Beim Zylinder liegt die Fläche
  zwischen dem unteren und dem oberen Kreis. Beim Torus ist auch die Gegenrichtung periodisch:
  Zwischen zwei Breitenkreisen liegen zwei Bänder (innen und außen herum). Welches gemeint ist,
  sagt der Umlaufsinn. Der Rand, der sich (um die Außennormale) gegen den Uhrzeigersinn windet,
  ist der Anfang, und das Band läuft von ihm aus in wachsender Gegenrichtung bis zum anderen
  Rand. Die Naht bekommt genau diesen Parameterbereich.
- **Eine Suche für beide Richtungen.** Die heutigen Hilfen für den Nahtwinkel (`u_track`,
  `crossings`, `free_angle`, `vertex_on`, `hole_ranges`, `u_extent`) rechnen nur in `u`. Sie
  bekommen die Richtung als Parameter (0 = `u`, 1 = `v`). Für `u` ändert sich nichts. Der
  Mindestabstand zu Löchern wird beim Rohrbogen am Rohrradius gemessen statt am Abstand zur
  Achse.
- **Nahtkurven:** Meridian `Curve::Circle` um die Rohrmitte beim Winkel `u_s` (Ebene durch die
  Achse, Parameter = `v`, wie in `revolve.rs`). Breitenkreis `Curve::Circle` um die Achse auf
  Höhe `minor · sin v_s` mit Radius `major + minor · cos v_s` (Parameter = `u`).
- **Meldungen:** keine neuen Texte. Passt ein Torus nicht zu den drei Formen, gibt es die
  bisherigen Meldungen („… ohne Naht wird noch nicht unterstützt.“, „Der Rand … schließt sich
  nicht …“, „… lässt sich keine Naht hinzufügen.“).

## Prüfung

- Von Hand geschriebene STEP-Proben in `tests/data/seamless_torus.step`, mit Volumen nach
  Handrechnung (Pappus/Guldin):
  - Welle mit gerundeter Kante (Zylinder, Ebene, Torusband außen herum),
  - Bohrung mit gerundetem Rand (Torusband innen herum, Band über `v = π`),
  - Rohrbogen 90° mit zwei Deckeln (auch mit einem Rohrkreis, der für die Naht geteilt wird),
  - voller Ring,
  - Gegenproben: ein Kreis falsch herum, beim Band und beim Bogen (Meldung, kein Körper).
- Nachtrag beim Bau: Eine Probe „Band mit Querbohrung“ ist von Hand nicht sinnvoll zu
  schreiben, weil die Schnittkurven von Bohrung und Torus Freiformkurven sind. Der Abstand zu
  Löchern läuft durch dieselbe Suche wie beim Zylinder und ist dort geprüft. Ein Torus, dessen
  Ränder alle Löcher sind, wird wie bisher als gewöhnliches Stück weitergegeben; die späteren
  Prüfungen (Seitenprobe, Volumen) entscheiden über ihn.
- Die bisherige Probe „Ring“ in `seamless.step` (Torus ganz ohne Rand) kommt jetzt an; der Test
  `unsupported_seamless_torus_is_skipped` wird zur Volumenprobe.
- **Handtest** (auf Dennis' Go): Import der Proben in der App, Ansicht und Volumen. Wenn Dennis
  eine STEP-Datei aus Fusion (Welle mit gerundeter Kante, Rohrbogen) exportieren kann, kommt sie
  als erste echte Fremddatei dazu. Ohne sie bleibt dieser Punkt in den offenen Punkten.

## Dem Bauenden überlassen

Aufteilung der Hilfsfunktionen, Namen, genaue Toleranzen innerhalb der heutigen Konstanten.
