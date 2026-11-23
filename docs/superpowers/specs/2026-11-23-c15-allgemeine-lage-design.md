# C15: Schnitte in allgemeiner Lage (Kurzentwurf)

Freigegeben von Dennis am 2026-11-23 („Ja, so bauen“); Dennis wählte zuerst „Allgemeine Lage“
(vor Schnittansicht mit Freiform und den Lücken bei Freiform).

## Warum

Ebene, Zylinder, Kegel, Kugel und Torus schneidet der Kernel nur, wenn Geraden, Kreise oder ebene
Kurven entstehen. Sonst: „Der Schnitt von … mit … wird noch nicht unterstützt“ – etwa bei einer
schrägen Bohrung in ein Rohr, einer schrägen Ebene durch Kegel oder Torus, Zylinder/Kegel/Kugel
neben der Achse des anderen, Torus gegen Zylinder, Kugel oder Torus. Fusion kann das.

## Entwurf

1. Liefert der analytische Weg `Unsupported`, wird eine der beiden Flächen **exakt** als NURBS
   dargestellt (rationale Drehfläche aus Strecke bzw. Kreis) und der vorhandene numerische
   Schnittrechner (`intersect_nurbs`) gerufen.
2. Welche Fläche: Rang Torus > Kegel > Zylinder > Kugel; die Ebene nie. Bei gleichem Rang die erste.
3. Begrenzung: Zylinder und Kegel über den Hüllquader ihrer Fläche (Projektion auf die Achse,
   mit Zugabe); der Kegel nie über die Spitze hinaus (auf der Seite seiner Fläche). Torus voll
   (in beiden Richtungen geschlossen).
4. Danach wie bei Freiformkörpern: Schnittkurven als NURBS, Toleranz `pipeline_tol(…, true)`,
   gleiche Arbeitsgrenze und Meldungen. Paare, die schon exakt gehen, bleiben auf ihrem Weg.
5. Nur der Boolesche Abgleich (`imprint`) nutzt den Ersatzweg; Rundungen und Ebenenschnitt
   (Schnittansicht) bleiben unverändert.

## Folgen

Neue Fälle auf Freiform-Genauigkeit (etwa 1e-5 mm statt 1e-6 mm), etwa 0,5–1 s je Operation.

## Verworfen

- Eigene exakte Formeln je Paar: viele Raumkurven vierten Grades, sehr viele Einzelfälle.
- Schnittrechner für zwei analytische Flächen umbauen: hängt an Bézier-Flicken, großer Umbau.

## Prüfung

Boolesche Tests: schräge Bohrung durch ein Rohr, Zylinder schräg durch Torus, schräge Ebene
(Quader) durch Kegel und Torus, Kugel mit Bohrung neben der Mitte, Kegel gegen Zylinder. Jedes
Ergebnis gültig und geschlossen; Volumen über Einschluss–Ausschluss (V(A∪B) + V(A∩B) = V(A) + V(B),
V(A∖B) = V(A) − V(A∩B)). Volle Läufe Windows und WSL, Prüfer, Handtest nur nach Go.
