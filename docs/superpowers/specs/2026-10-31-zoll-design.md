# Teilprojekt Zoll – Entwurf

Stand: 2026-10-31. Dennis hat die Einzelheiten an Claude übergeben („selber entscheiden“); alle
Festlegungen unten sind Claudes Entscheidung. Grundlage ist die Bestandsaufnahme vom selben Tag
(rund 12 feste „mm“ in Formatierern, rund 30 Meldungen mit „mm“ in den `.ftl`-Dateien, rund
115 Testerwartungen mit „mm“).

## Warum

PlanoraCAD zeigt und nimmt Längen nur in Millimeter. Für Nutzer in den USA (und für Teile, die
in Zoll konstruiert sind) fehlt eine Zoll-Anzeige. Beim Entwurf der Sprachen (2026-10-27) wurde
das bewusst als eigenes Teilprojekt ausgeklammert.

## Ziel

1. **Einstellung „Maßeinheit“** unter Einstellungen → Darstellung, neben der Sprache:
   „Millimeter“ oder „Zoll“. Gilt für das ganze Programm, wird in `config.json` gespeichert
   (wie die Sprache), Standard Millimeter. Umschalten wirkt sofort, ohne Neustart.
2. **Anzeige:** Alle Längen, Flächen und Volumen im Modell erscheinen in der gewählten Einheit:
   Eigenschaftsfeld, Skizzenmaße und Bedingungsliste, Formelfelder („= 0,787 in“), Dialoge,
   Größen im STEP-Import, Hover-Zeilen der Verlaufsleiste und alle Meldungen mit Längen
   („höchstens … mm“ → „höchstens … in“).
   - Zeichen: `mm`, `mm²`, `mm³` bzw. `in`, `in²`, `in³` – in beiden Sprachen „in“.
   - Nachkommastellen: wo heute 2 Stellen in mm stehen, stehen in Zoll 3 (ein Tausendstel Zoll
     ≈ 0,025 mm, also ähnlich fein wie 0,01 mm). Dezimalzeichen folgt wie heute der Sprache.
   - Gitter im Ansichtsfenster: Zehnerschritte in Zoll (0,1 in, 1 in, 10 in …) statt in mm.
3. **Eingabe:** Wer in Zoll arbeitet, tippt Zoll.
   - Eine reine Zahl in einem Längenfeld bedeutet die gewählte Einheit.
   - Der Formelleser kennt Einheiten direkt hinter einer Zahl: `mm`, `cm`, `m`, `in`
     (z. B. „2 in“, „50mm“, „breite + 1 in“). So kann man jederzeit beide Einheiten mischen.
   - Positionsfelder (Ziehwerte) im Eigenschaftsfeld zeigen und nehmen die gewählte Einheit;
     Schrittweite beim Ziehen 0,02 in statt 0,5 mm.
4. **Zeichnungen:** Jede Zeichnung hat eine eigene Maßeinheit (mm oder Zoll), damit dieselbe
   Zeichnung auf jedem Rechner gleich aussieht. Eine neue Zeichnung übernimmt die Einstellung
   beim Anlegen; ändern lässt sie sich in den Blattangaben.
   - Maßzahlen werden umgerechnet (ohne Einheitenzeichen, wie heute). Neue Maße bekommen in
     Zoll 2 Nachkommastellen statt 1.
   - Toleranzen in der Maßbearbeitung gelten in der Einheit der Zeichnung.
   - Eine Zeichnung in Zoll trägt im Schriftfeld den Vermerk „Maße in Zoll“ / „Dimensions in
     inches“. Zeichnungen in mm bleiben unverändert (kein neuer Vermerk).

## Nicht-Ziele

- Brüche als Anzeige (1/8″), Fuß, Mikrometer, mils. Eingabe von „1/8 in“ geht trotzdem, weil
  der Formelleser teilen kann.
- Eine Einheit pro Dokument oder pro Körper. Die Einheit ist eine Einstellung des Programms;
  nur Zeichnungen speichern ihre eigene.
- Papierformate nach ANSI (Letter, B, C …) und Blattmaße in Zoll. Blatt, Strichstärken,
  Schraffurabstand, Schriftfeld-Maße bleiben Millimeter (sie gehören zum Papier, nicht zum Teil).
- Export-Dateien: STL/OBJ/3MF/glTF/STEP schreiben unverändert in ihren festen Einheiten;
  DXF/SVG/PDF der Zeichnungen bleiben Blätter in mm. Die Einheitenwahl beim Import bleibt
  wie sie ist (sie beschreibt die Datei, nicht die Anzeige).
- Runde Vorgabewerte in Zoll (z. B. 0,5 in statt 10 mm beim neuen Quader). Vorgaben bleiben
  intern gleich und erscheinen umgerechnet; in den offenen Punkten als Folgeidee vermerkt.
- Änderungen am Kernel.

## Anti-Ziele

- **Kein stiller Einheitenwechsel.** Ein Wert darf nie mit dem falschen Zeichen erscheinen
  (Zoll-Zahl mit „mm“ oder umgekehrt), und eine gespeicherte Formel darf ihren Wert nicht
  ändern, wenn jemand die Einstellung umstellt. Lieber eine Stelle sichtbar in mm lassen als
  falsch beschriften; jede solche Stelle steht dann in den offenen Punkten.
- Vorhandene Dateien und Zeichnungen sehen in der Einstellung Millimeter genau aus wie heute;
  das Dateiformat steigt nur, wenn eine Zeichnung Zoll benutzt.
- Ein unverändert bestätigtes Feld schreibt nichts um (kein Rundungsverlust durch Hin- und
  Zurückrechnen).

## Wie (Claudes Festlegungen)

- **Intern bleibt alles in mm.** Kernel, Dokument, Parameterwerte und Dateien rechnen weiter
  in Millimeter. Die Einheit wirkt nur an den Rändern: Anzeige, Eingabe, Zeichnungsmaße.
- **Ein Ort für die Einheit:** ein kleiner Typ `LengthDisplay`/`DisplayUnit { Millimeter, Zoll }`
  neben der Sprache in `planora-i18n` (globaler Wert wie `Lang`, mit Testüberschreibung pro
  Thread), dazu Formatierer für Länge, Fläche, Volumen und Ziffern-mit-Zeichen. Alle Stellen,
  die heute „mm“ selbst schreiben, gehen über diese Formatierer. Die `.ftl`-Meldungen mit
  „mm“ bekommen das Zeichen als Variable (`{ $unit }`) und den umgerechneten Wert.
- **Gespeicherte Formeln bleiben eindeutig.** Im gespeicherten Formeltext bedeutet eine reine
  Zahl (ohne Zeichen) immer Millimeter – genau wie heute, damit alte Dateien gleich bleiben.
  - Bestätigt jemand in der Einstellung Zoll ein geändertes Längenfeld, wird der Text vor dem
    Speichern so umgeschrieben, dass jede Zahl, die als Länge zählt, „in“ trägt („2“ →
    „2 in“, „breite + 1“ → „breite + 1 in“). Faktoren bleiben reine Zahlen („breite * 2“).
    „Als Länge zählen“ heißt: die Zahl ist das ganze Ergebnis eines Längenfelds oder steht bei
    `+`, `-`, `min`, `max` neben einer Länge – dieselbe Regel, nach der heute eine reine Zahl
    zur Länge passt.
  - Angezeigt wird in Zoll: ein reiner Zahlenwert umgerechnet („50“ → „1,969“ im Feld mit
    Zeichen „in“); jede andere Formel so, wie sie gespeichert ist, wobei reine Längenzahlen
    sichtbar „mm“ bekommen („breite + 5 mm“), damit nichts falsch gelesen wird.
  - In der Einstellung Millimeter ändert sich am Speichern nichts.
- **Zeichnungseinheit im Dokument:** neues Feld an `Drawing` (Standard mm, beim Lesen alter
  Dateien mm). Dateiformat-Version steigt nur, wenn eine Zeichnung Zoll nutzt (wie bei den
  STEP-Körpern).
- **Texte:** neue Schlüssel in `de.ftl` und `en.ftl` (Einstellung, Einheitennamen, Vermerk im
  Schriftfeld); kein neuer deutscher Text als Rust-Literal.

## Prüfung

- Formatierer: Werte in mm und Zoll, beide Sprachen, Fläche und Volumen (25,4² bzw. 25,4³).
- Formelleser: Einheiten hinter Zahlen, Umschreiben beim Bestätigen, Anzeige in Zoll,
  unverändert bestätigt bleibt zeichengleich, Umstellen der Einstellung ändert keinen Wert.
- Oberflächentests (wie die vorhandenen egui-Tests) in Zoll für Eigenschaftsfeld, Formelfeld,
  Skizzenmaß, Meldung „höchstens …“, STEP-Importgrößen, Einstellungsfenster.
- Zeichnung in Zoll: Maßzahlen umgerechnet, Vermerk im Schriftfeld, Speichern/Laden,
  Dateiformat nur bei Zoll höher; vorhandene Zeichnungsproben bleiben bytegleich.
- Alle vorhandenen Tests bleiben in der Einstellung Millimeter unverändert grün.
- Handtest (auf Dennis' Go): Umschalten, Quader in Zoll anlegen, Skizzenmaß in Zoll, Zeichnung
  in Zoll als PDF.

## Dem Bauenden überlassen

Namen, Aufteilung der Formatierer, wie das Umschreiben im Formelleser die Stellen der Zahlen
findet, genaue Lage des Auswahlfelds im Einstellungsfenster und in den Blattangaben.
