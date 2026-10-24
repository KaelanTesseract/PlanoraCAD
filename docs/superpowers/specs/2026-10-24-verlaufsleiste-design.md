# Verlaufsleiste und neuer Feature-Baum – Entwurf

Stand: 2026-10-24. Entwurf zur Prüfung durch Dennis. Alles, was mit „Mein Vorschlag“ markiert ist, ist eine Annahme von Claude und nicht vom Nutzer entschieden.

## Ziel

Der Verlauf der Schritte (Quader, Bohrung, Abrundung …) wird wie in Fusion als Leiste am unteren Rand gezeigt. Ein Marker lässt sich hin und her schieben. Der Feature-Baum links zeigt nur noch Körper, Skizzen und Zeichnungen. Das Ändern eines früheren Schrittes (Doppelklick) berechnet alle späteren Schritte neu.

Gründe: Der Verlauf im Baum ist unübersichtlich. Die Leiste ist aus Fusion bekannt und der Kern kann Rückrollen und Neuberechnen bereits.

## Entschieden (vom Nutzer)

1. Leiste unten wie in Fusion: Abspielknöpfe, ein Symbol je Schritt, Marker zum Verschieben, Schritte hinter dem Marker grau.
2. Die Leiste gehört zu **einem gewählten Körper**; es gibt **einen Marker je Körper** (Variante B).
3. Springt der Marker nach vorn, werden die zurückgestellten Schritte neu berechnet.
4. **Doppelklick auf ein Symbol** öffnet die Einstellungen des Schritts (z. B. Höhe beim Extrudieren). Nach dem Ändern rechnet das Programm alle abhängigen Schritte neu.
5. Der Verlauf verschwindet aus dem Feature-Baum.
6. Feature-Baum nach Entwurf A (Ordner: Körper, Skizzen, Zeichnungen, Ursprung; Auge je Zeile), aber **mit den Körperfarben aus Entwurf C**. Dieselbe Farbe steht am Körper im Baum und an dessen Schritten in der Leiste.
7. Der Baum hat einen durchsichtigen Hintergrund. Die Durchsichtigkeit ist in den **Einstellungen** regelbar.

## Mein Vorschlag (nicht entschieden)

- Ein Schritt, der mehrere Körper verbindet (Vereinigen, Schneiden), erscheint in der Leiste jedes beteiligten Körpers.
- Neue Aktionen an einem zurückgesetzten Körper sind gesperrt, bis dessen Marker am Ende steht. Die Oberfläche sagt das mit einem Hinweis und bietet „Zum Ende springen“ an. (Fusion fügt stattdessen an der Markerstelle ein; das wäre ein eigenes Teilprojekt.)
- Voreinstellung der Durchsichtigkeit: 40 % durchsichtig, regelbar von 0 bis 80 %. Mehr als 80 % lässt die Schrift unlesbar werden.
- Farben der Körper: feste Reihe aus der Graphit-Palette, der Körper behält seine Farbe, solange er existiert. Nutzer kann die Farbe später ändern (nicht Teil dieses Teilprojekts).
- Die Leiste hat im Hell- und Dunkelmodus die Panel-Farbe der anderen schwebenden Panels, keine eigene Durchsichtigkeit.

## Kern: Marker je Körper (Freigabe des Nutzers nötig)

Heute hat das Dokument eine geordnete Liste der Schritte und **einen** Marker: Schritte ab dem Marker sind „nicht berechnet“, ihre Ergebnisse bleiben im Zwischenspeicher, das Zurückholen ist billig. Schritte, die von fehlerhaften oder ausgesetzten Schritten abhängen, werden übersprungen.

Mein Vorschlag, mit der kleinsten Änderung:

- **Rückrollstelle statt zweitem Marker.** Neu ist eine Liste `Rückrollstellen` (Schritt-Ids). Eine Rückrollstelle bei Schritt *c* setzt *c* und alle Schritte zurück, die (auch über Umwege) von *c* abhängen. Das sind genau die späteren Schritte desselben Körpers und alles, was ihn weiterverwendet.
- Der Marker eines Körpers steht also an der Rückrollstelle in dessen Kette. Fehlt eine Stelle, ist der Körper ganz berechnet.
- Der **globale Marker bleibt** wie er ist (Dateien, Dialoge, Tests bleiben gültig). Ein Schritt ist aktiv, wenn er vor dem globalen Marker liegt **und** nicht zurückgesetzt ist.
- Zurückgesetzte Schritte bekommen den Status „nicht berechnet“ (wie heute hinter dem Marker), nicht „übersprungen“. Ergebnisse bleiben im Zwischenspeicher, Vorwärtsspringen ist billig, solange sich nichts geändert hat.
- **Verbundene Körper:** Liegt die Rückrollstelle vor einem Vereinigen, ist das Vereinigen zurückgesetzt. Die beiden Eingangskörper sind dann wieder einzeln sichtbar (das macht die vorhandene Sichtbarkeitsberechnung schon). Die Kette des zweiten Körpers bleibt bis zum Vereinigen aktiv.
- Nebenwirkungen: Stückliste, Zeichnungen und Abmessungen, die zurückgesetzte Körper verwenden, verhalten sich wie bei heute zurückgestellten Schritten (Status „nicht berechnet“, keine neuen Fehler).
- **Datei:** Die Rückrollstellen werden gespeichert, damit der Stand beim Öffnen derselbe ist. Das ändert das Dateiformat (neue Formatversion; alte Dateien öffnen sich unverändert, neue lassen sich mit älteren Programmständen nicht mehr öffnen). Mein Vorschlag: speichern, weil sonst der Nutzer nach dem Öffnen nicht dort ist, wo er aufgehört hat.
- Jede Änderung geht über den bestehenden Weg (`set_marker` bleibt; neu `set_rollback(body, step)` und `clear_rollbacks`). Änderungen zählen als Dokumentänderung (ungespeichert-Hinweis) wie heute der Marker.

Betroffene Bereiche: `planora-feature` (Modell, Neuberechnung, Schnappschuss), `planora-project` (Format), `planora-app` (Leiste, Baum, Dialoge, Einstellungen). Der Geometriekern (`planora-kernel`) ändert sich nicht.

## Oberfläche

### Leiste (neues Panel am unteren Rand, über der Werkzeugleiste)

- Links die Abspielknöpfe: Anfang, ein Schritt zurück, Abspielen (schrittweise vorwärts mit festem Takt), ein Schritt vor, Ende.
- Dann ein Symbol je Schritt des gewählten Körpers (in der Körperfarbe), der Marker als senkrechte Linie mit Griff.
- Marker ziehen oder Pfeiltasten (links/rechts, wenn die Leiste den Fokus hat). Zurückspringen setzt die Rückrollstelle, Vorwärtsspringen hebt sie auf oder verschiebt sie.
- Schritte hinter dem Marker sind abgeblendet.
- Klick wählt den Schritt (und die Zeile im Baum). Doppelklick öffnet den Dialog des Schritts (vorhandener `feature_dialog`). Hinweis beim Darüberfahren: Name und wichtigste Maße.
- Fehlerhafte Schritte tragen ein Warnzeichen.
- Ohne gewählten Körper zeigt die Leiste den Hinweis „Körper wählen“, die Knöpfe sind abgeblendet.
- Überläuft die Leiste die Breite, scrollt sie waagerecht, der Marker bleibt sichtbar.

### Feature-Baum (links, durchsichtig)

- Dokument, Ordner Körper, Skizzen, Zeichnungen, Ursprung. Auge je Zeile (wie heute für die Sichtbarkeit).
- Körperzeilen tragen ein farbiges Quadrat; Klick wählt den Körper und schaltet die Leiste um.
- Durchsichtiger Hintergrund; Schrift bleibt lesbar (dunkler Schatten oder Fläche mit Mindestdeckkraft, Entscheidung beim Bau).

### Einstellungen

- Neuer Abschnitt „Darstellung“ mit dem Regler „Durchsichtigkeit der Panels“ (0–80 %, Voreinstellung 40 %). Gilt für den Feature-Baum; weitere Panels können später folgen (mein Vorschlag: zunächst nur der Baum).
- Wird im selben Ort wie das Projektverzeichnis gespeichert.

## Nicht-Ziele

- Einfügen neuer Schritte an der Markerstelle (Fusion-Verhalten).
- Umordnen von Schritten per Ziehen.
- Weichzeichner hinter dem Panel (mit egui nicht möglich).
- Eigene Farben je Körper wählen.
- Änderungen am Geometriekern.

## Anti-Ziele

- Durch das Zurückspringen darf nichts verloren gehen (keine gelöschten Schritte, keine geänderten Maße).
- Nach dem Vorwärtsspringen muss das Ergebnis dasselbe sein wie vor dem Zurückspringen, wenn nichts geändert wurde.
- Der Baum darf nicht unlesbar werden, auch nicht bei hellem Hintergrund und 80 % Durchsichtigkeit.

## Randfälle, die Tests brauchen

- Rückrollstelle bei einem Schritt, auf dem ein Vereinigen eines zweiten Körpers aufbaut.
- Rückrollstelle bei Skizze und Extrudieren (Körper hängt an Skizze).
- Globaler Marker und Rückrollstelle zugleich.
- Schritt löschen, der eine Rückrollstelle ist oder hinter ihr liegt.
- Parameter eines zurückgesetzten Schritts ändern, dann vorwärts springen.
- Speichern und Laden mit Rückrollstellen; Laden alter Dateien.
- Rückgängig/Wiederholen über Marker- und Rückrollbewegungen.
- Körper ohne Schritte, Körper aus STEP-Import oder Netz (kein Verlauf).
- Sehr lange Ketten (Leiste scrollt).

## Etappen (Vorschlag)

1. Kern: Rückrollstellen in `planora-feature` mit Tests, Schnappschuss und Format.
2. Leiste (ohne Doppelklick) und Körperwahl.
3. Doppelklick, Hinweise, Tastatur.
4. Neuer Feature-Baum mit Farben, Durchsichtigkeit, Einstellungen.
5. Bereinigung: alter Verlauf im Baum entfernen, Dokumentation.

Jede Etappe mit Sichtprüfung im echten Fenster.
