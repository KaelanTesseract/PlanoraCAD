# Kleinpunkte 18, 21 und 16 (Kurzentwurf)

Freigegeben von Dennis am 2026-11-22 („Ja, so bauen“). Die Punkte stammen aus der Prüfung der
offenen Punkte vom 2026-11-22 (`offene-punkte.md`, Abschnitte „Kleinpunkte nach den Fusion-Resten“
und Kernel).

## Absicht

Drei kleine Fehler beheben, die Nutzer wirklich merken: eine irreführende Meldung am Feld, Maße,
die in Schnittansichten beim Ausblenden verloren gehen, und Ebenen, die in Grenzfällen kippen oder
weit weg landen. Normale Fälle rechnen genau wie bisher; alte Dateien ändern sich nicht.

## 18. Genauer Grund am Feld von Rundung und Fase

- Heute: Gibt es zum getippten Wert keinen Plan, zeigt das Feld „Wert zu groß“, egal warum.
- Neu: `KindGripState.over` trägt Feld und Grund:
  - Wert unter einem Rundungsschritt (0,01 mm, Zoll 0,001 in) → „Wert zu klein“.
  - Sonst: passt ein Schritt, der getippte Wert aber nicht → „Wert zu groß“.
  - Sonst (kein Wert passt: Ellipse/Spline ohne Lösung, unpassender Fasenwinkel) → „Passt hier
    nicht“.
- Kosten: eine Probe mehr je Bild, nur solange kein Plan da ist.

## 21. Maße in Schnittansichten ruhen an ausgeblendeten Körpern

- Heute: Schnittansichten schneiden nur eingeblendete Körper; ein Maß an einem ausgeblendeten
  Körper gilt als verloren (rot).
- Neu: Nur wenn in einer Schnittansicht ein Maß nicht aufgeht und es ausgeblendete exakte Körper
  gibt, werden diese an der Ebene der Ansicht zusätzlich geschnitten. Geht das Maß gegen die
  geschnittenen ausgeblendeten Körper auf, ruht es (fällt aus `by_view`, steht in `resting`), wie
  in Ansichten ohne Schnitt.
- Verworfen: Prüfen gegen die ungeschnittenen Körper (billiger, verfehlt Maße an Schnittflächen).

## 16. Ebenen in Grenzfällen (`planora-feature/src/planes.rs`)

- Drei Punkte: kollinear, wenn der Abstand des dritten Punkts zur Geraden durch die beiden am
  weitesten entfernten Punkte ≤ `tol.linear` ist (heute: Dreiecksfläche ≤ `tol.linear²`, bei langen
  Seiten viel zu klein). Gültige Fälle behalten Ursprung und Achsen wie bisher.
- Mittelebene: Läge der Punkt der Schnittgeraden weiter als 1 km (`1e6` mm) von den Ursprüngen der
  beiden Ebenen, gilt das Paar als parallel (Mitte dazwischen, Lage von `a`). Sonst unverändert.

## Ansätze

Je Punkt gibt es einen sinnvollen Weg; der einzige echte Wahlpunkt war 21 (genau schneiden statt
gegen ungeschnittene Körper prüfen), entschieden wie oben.

## Nachweis

Je Punkt ein Test, der vorher scheitert; volle Prüfung unter Windows und WSL; Handtest nur nach
Go.
