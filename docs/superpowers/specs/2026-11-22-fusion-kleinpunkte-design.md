# Kleinpunkte nach den Fusion-Resten – Entwurf

Stand 2026-11-22. Punkt B der offenen Punkte. Von Dennis freigegeben („ja“, Maße eingeschlossen).

## Warum

Der Handtest von `fix/fusion-reste` (51da8974) hat Kleinpunkte gezeigt: Griffe in der Skizze ohne
Obergrenze, viele Nachkommastellen in Zoll, Felder über der rechten Leiste, rote Meldungen an
ausgeblendeten Körpern.

## Ziele, Nicht-Ziele

- Ziel: die vier Punkte unten.
- Nicht-Ziel: Fang und feste Pixellänge der Ebenen-Griffe (Fusion: feste Bildschirmgröße, kein
  Einrasten beim Ziehen; nur in der Doku festhalten). SVG-Einfügen: kein Fehler (Mitte der Datei
  auf den Ursprung, dann verschieben), nur Doku.
- Anti-Ziel: andere Zahlen in bestehenden Dokumenten, neues Dateiformat.

## Wege

1. Ein Zweig `fix/fusion-kleinpunkte`, je Punkt ein Commit (gewählt).
2. Je Punkt ein Zweig: kein Gewinn, kleine Punkte.

## Entwurf

1. **Obergrenze am Griff:** Beim Ziehen bleibt der Wert am größten, der noch passt (Suche zwischen
   dem Wert beim Greifen und dem gezogenen); die Vorschau verschwindet nicht, der Zug endet nicht.
   Ein getippter zu großer Wert zeigt am Feld „Wert zu groß“. Empfindlichkeit bleibt (der Griff
   bleibt unter dem Zeiger auf dem Bogen).
2. **Zoll:** Der gezogene Wert wird in der Anzeigeeinheit gerundet: mm auf 0,01, Zoll auf 0,001.
3. **Felder am Zeiger:** Würden sie eine schwebende Leiste (links, rechts, Skizzenleiste)
   überdecken, klappen sie auf die andere Seite des Zeigers. Gilt für alle Werkzeuge.
4. **Ausgeblendete Körper ruhen in Zeichnungen:**
   - Positionsnummern an einem ausgeblendeten (nicht gelöschten) Körper werden nicht gezeichnet
     und nicht als verloren gezählt.
   - Maße, deren Bezüge nur an ausgeblendeten Körpern hängen, werden nicht gezeichnet (heute
     stehen sie frei ohne Kante). Sie zählen weder als aufgelöst noch als verloren.
   - Der Export (PDF, DXF, SVG) lässt ausgeblendete Körper weg wie der Bildschirm.
   - Wieder eingeblendet kommt alles zurück.

## Dem Bauenden überlassen

Name der Hilfsfunktionen, Suche (Bisektion), Commit-Aufteilung.
