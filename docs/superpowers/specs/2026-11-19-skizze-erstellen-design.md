# Skizze erstellen wie Fusion – Entwurf

Stand: 2026-11-19, Ausgang master `ebccdd7e`. Zweig: `feature/skizze-erstellen` (neu von master, eigener Worktree).

## Absicht

Eine Skizze soll so entstehen wie in Autodesk Fusion („Create Sketch“, deutsch „Skizze erstellen“).
Heute legt das Menü „Erzeugen“ mit vier festen Einträgen sofort eine Skizze an: „Skizze auf
XY-/XZ-/YZ-Ebene“ und „Skizze auf gewählter Fläche“. Der letzte Eintrag geht nur, wenn vorher eine
Fläche oder Ebene gewählt ist. In Fusion gibt es einen einzigen Befehl. Man ruft ihn auf und wählt
dann die Ebene per Klick im Bild. Wer Fusion kennt, soll in PlanoraCAD ohne Nachdenken eine Skizze
anlegen können.

Erfolg heißt: Ein Klick auf das Symbol „Skizze erstellen“ oben, dann ein Klick auf eine Ebene oder
eine ebene Fläche, und die Skizze ist offen. Die Ansicht blickt so auf die Ebene wie in Fusion.

## Ziele

1. **Ein Befehl „Skizze erstellen“** (en „Create Sketch“). Er ersetzt die vier alten Einträge im
   Menü „Erzeugen“ (mit Dennis abgestimmt am 2026-11-19, Variante a).
2. **Symbol als erstes in der unteren Werkzeugleiste** (Befehlsfeld, `shell_panels::bottom_field`),
   direkt anklickbar und nicht in einem Untermenü, vor Quader, Zylinder usw. und mit Trennstrich
   davor wie in Fusion (Dennis' Wahl am 2026-11-19). Beim Überfahren erscheinen Name und Taste.
   Gesperrt wie die übrigen Knöpfe dort (`edit_lock`).
3. **Tastenkürzel:** eigene `Action` in der Tastenbelegung, in den Einstellungen belegbar. Ab Werk
   ist keine Taste belegt, wie in Fusion.
4. **Vorauswahl:** Ist beim Aufruf schon eine Grundebene, eine Konstruktionsebene oder eine ebene
   Körperfläche gewählt, legt der Befehl die Skizze sofort darauf an und öffnet sie, ohne Wahlmodus.
5. **Wahlmodus**, wenn nichts Passendes gewählt ist:
   - Die drei Grundebenen erscheinen halbdurchsichtig im Bild, auch wenn „Ursprung“ oder einzelne
     Grundebenen im Baum ausgeblendet sind. Nach dem Modus gilt wieder die alte Sichtbarkeit.
   - Die Ebene oder Fläche unter dem Zeiger wird hervorgehoben (wie heute beim Überfahren).
   - Wählbar sind die Grundebenen, sichtbare Konstruktionsebenen und ebene Flächen sichtbarer
     Körper. Kanten, Ecken und Skizzenkurven nicht.
   - Ein Klick auf eine Grund- oder Konstruktionsebene im Baum zählt wie ein Klick im Bild.
   - Die Statuszeile zeigt den vorhandenen Hinweis `state-pick-sketch-plane` („Ebene oder ebene
     Fläche wählen“), wie bei „Skizzenebene neu festlegen“.
   - Drehen, Schieben und Zoomen der Ansicht gehen weiter.
   - Eine gekrümmte Fläche oder die Fläche eines Netz-Körpers gibt nur eine Meldung, und der Modus
     bleibt.
6. **Abbruch ohne Rest:** Esc, Rechtsklick in die Ansicht, ein anderer Befehl oder ein Ebenen-Fenster
   beenden den Modus. Nichts wird angelegt, kein Rückgängig-Schritt, die Auswahl bleibt leer.
7. **Ein Rückgängig-Schritt:** Anlegen und Öffnen sind wie heute ein Schritt (`add_sketch` und
   `open_sketch`).
8. **Bei offener Skizze** schließt „Skizze erstellen“ sie erst (wie `try_close_sketch`) und startet
   dann Vorauswahl oder Wahl. Scheitert das Schließen, bleibt alles, wie es war, und die Meldung
   erscheint.
9. **Blickrichtung nach dem Start wie Fusion mit „Z nach oben“:** XY von oben (+Z, Bild-oben +Y),
   XZ von vorn (−Y, Bild-oben +Z), YZ von rechts (+X, Bild-oben +Z). Das tut `open_sketch` heute
   schon über `StandardPlane::frame()`. Ein Test sichert es für alle drei ab.
10. **„Skizzenebene neu festlegen“** zeigt im Wahlmodus ausgeblendete Grundebenen ebenfalls, wie
    Fusion.
11. **Rechtsklick im Baum** auf Grund- und Konstruktionsebenen behält „Skizze erstellen“. Das gibt
    es in Fusion auch.

## Nicht-Ziele

- Keine Skizze auf Skizzenprofilen oder gekrümmten Flächen.
- Keine neue Anzeige der Ebenen außerhalb des Wahlmodus.
- Keine Änderung an der Einstellung „Z nach oben“. Eine Einstellung „Y nach oben“ wie in Fusion
  gibt es in PlanoraCAD nicht und kommt hier nicht hinzu.
- Kein Symbol im oberen Menüfeld: Das Symbol steht unten in der Werkzeugleiste (Ziel 2).
- Kein neues Dateiformat. Das Dokument enthält nichts Neues.

## Anti-Ziele

- Ein abgebrochener Wahlmodus darf nichts hinterlassen: keine Skizze, keinen Verlaufsschritt, keine
  geänderte Sichtbarkeit, keine hängende Hervorhebung.
- Die eingeblendeten Grundebenen dürfen die Sichtbarkeit im Dokument nicht ändern. Sie werden nur
  gezeichnet und getroffen, nicht in `Visibility` geschrieben.
- „Skizzenebene neu festlegen“ darf sich außer Ziel 10 nicht anders bedienen als heute.

## Einschränkungen

- Texte nur über `tr!`, Schlüssel in de und en. Die alten Schlüssel `menu-sketch-xy`,
  `menu-sketch-xz`, `menu-sketch-yz` und `menu-sketch-on-face` entfallen, sofern sie sonst niemand
  nutzt.
- Gesperrt wie heute das Anlegen einer Skizze: in alten Versionen, während einer offenen Rückfrage
  (`edit_lock`), wenn die Verlaufsleiste den Körper der gewählten Fläche zurückgesetzt hat
  (`sketch_on_face_blocked`, nur für die Vorauswahl). Im Zeichnungsmodus ist das Symbol ausgeblendet
  oder gedämpft wie die übrigen Modellbefehle.
- Die Werkzeugleiste muss bei 1920×1080 mit dem neuen Symbol weiter ganz sichtbar sein.

## Ansätze

1. **Gewählt: eigener Wahlmodus neben „Skizzenebene neu festlegen“.** `PendingPick` bekommt eine
   zweite Variante (etwa `NewSketch`). Treffer im Bild (`plane_click_at` mit `PickFilter::BASES`),
   Hervorhebung, Baum-Klick im Ordner „Ursprung“, Esc und Rechtsklick laufen schon über
   `pending_pick` und werden mitgenutzt. Nur das Ende unterscheidet sich: Statt
   `redefine_sketch_plane` folgen `add_sketch` und `open_sketch`. Wenig neuer Code, und beide
   Befehle verhalten sich gleich.
2. **Fenster mit Auswahlfeld** wie die Ebenen-Dialoge (`plane_pick`). Verworfen: Fusion zeigt beim
   Skizze-Erstellen kein Fenster, das wäre ein zusätzlicher Klick.
3. **Alte Menüeinträge behalten** als Abkürzungen neben dem neuen Befehl. Verworfen von Dennis
   (Variante b): Das weicht von Fusion ab.

## Festgelegtes Verhalten

| Lage beim Aufruf | Ergebnis |
|---|---|
| Grundebene gewählt (Baum) | Skizze auf ihr, sofort offen |
| Konstruktionsebene gewählt, berechnet | Skizze auf ihr (`new_sketch_on_plane`) |
| Ebene Körperfläche gewählt | Skizze auf ihr (`new_sketch_on_face`) |
| Nichts oder Unpassendes gewählt (Kante, Körper, gekrümmte Fläche, Skizze) | Auswahl leeren, Wahlmodus |
| Skizze offen | erst schließen, dann wie oben |

Im Wahlmodus: Ein Klick auf einen passenden Treffer legt an und öffnet. Scheitert das Anlegen (etwa
weil die Ebene gesperrt ist), erscheint die Meldung und der Modus endet.

## Klärungen aus der Prüfung (eigene Festlegungen)

- **Leeres Dokument:** Die Grundebenen haben im Modus die Größe aus `plane_rect_for` (halbe Kante
  mindestens 25 mm). Sie sind also auch ohne Körper gut sichtbar und treffbar, ohne neue Regel.
- **Ausgeblendete Konstruktionsebene als Vorauswahl:** Ist sie (etwa im Baum) gewählt und berechnet,
  entsteht die Skizze darauf wie heute bei „Skizze auf gewählter Fläche“. Im Wahlmodus treffen im
  Bild nur sichtbare Konstruktionsebenen, im Baum auch ausgeblendete.
- **Rechtsklick:** Wie bei „Skizzenebene neu festlegen“ beendet nur ein Rechtsklick ohne Ziehen den
  Modus (`secondary_clicked`). Drehen mit gedrückter Taste bleibt möglich.
- **Abbruch nach geschlossener Skizze:** Die vorher offene Skizze bleibt geschlossen. Ihr Schließen
  ist ein eigener, schon erledigter Schritt. „Ohne Rest“ heißt: Der neue Befehl legt nichts an.
- **Blickrichtung auf Konstruktionsebenen und Flächen:** bleibt wie heute. Neu getestet werden nur
  XY, XZ und YZ.
- **Tooltip ohne Taste:** Ist keine Taste belegt, zeigt das Symbol nur den Namen
  (`keymap::with_hint` tut das schon).

## Dem Bauenden überlassen

- Name und Form der neuen `PendingPick`-Variante und der `Action`. Ob die Endaktion in
  `pick_sketch_plane` verzweigt oder eine eigene Funktion bekommt.
- Wie die ausgeblendeten Grundebenen im Modus gezeichnet und getroffen werden (`origin_plane_rects`
  und `plane_strokes` fragen dann zusätzlich den Modus ab).
- Welches Symbol (Phosphor, wie die übrigen; etwa das bisherige „pencil-simple-line“).
- Tests: Vorauswahl je Art, Wahlmodus mit Treffer, Abbruch ohne Rest (Verlauf, Sichtbarkeit,
  Auswahl), ausgeblendete Grundebene treffbar, Blickrichtung XY/XZ/YZ, offene Skizze wird
  geschlossen.
- Ein Handtest bei 1920×1080 am Ende (nur nach Dennis' Go).
