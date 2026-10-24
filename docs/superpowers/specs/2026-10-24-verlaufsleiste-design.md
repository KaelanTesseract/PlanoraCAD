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

## Festlegungen nach der Bauherren-Prüfung (Claudes Entscheidungen, bitte prüfen)

1. **Eine Rückrollstelle je Körper.** `set_rollback(körper, schritt)` ersetzt die alte Stelle dieses Körpers. `clear_rollbacks(körper)` löscht nur dessen Stelle, `clear_all_rollbacks()` alle. Schritte anderer Körper, die vom zurückgesetzten abhängen, sind abgeleitet inaktiv; ihre Leiste zeigt sie abgeblendet, ohne eigene Stelle.
2. **Vereinigen/Schneiden** (geändert beim Bau, Aufgabe 3): Solange zwei Körper getrennt sind, hat jeder seine eigene Leiste A und B, und das Ziehen in A lässt die Kette von B unberührt. Nach dem Vereinigen gibt es die Eingangskörper nicht mehr als Körper (sie sind verbraucht). Es gibt dann **eine einzige lineare Leiste** für die Spitze des Vereinigens, über die ganze Kette in der Reihenfolge der Schrittliste. Wer dort den Marker zieht, ersetzt unter Umständen die Rückrollstelle des anderen Eingangs. Getrennte Leisten je Eingang eines vereinigten Körpers gibt es nicht (möglicher späterer Ausbau). Das Vereinigen ist erst wieder aktiv, wenn beide Eingangsketten bis dahin aktiv sind.
3. **Sperre je Körper:** Solange ein Körper zurückgesetzt ist, sind neue Aktionen an ihm gesperrt (Hinweis, „Zum Ende springen“). Erlaubt bleiben: Parameter bearbeiten (Doppelklick, auch bei zurückgesetzten Schritten), Löschen, Auge, Umbenennen. Andere Körper sind nicht betroffen.
4. **Rückgängig/Wiederholen:** Marker- und Rückrollbewegungen werden behandelt wie heute der globale Marker (der Bauer prüft das im Code und übernimmt das vorhandene Verhalten; keine neue Rückgängig-Art).
5. **Körperfarbe** wird aus der Id des ersten Schritts des Körpers abgeleitet (Palette der Reihe nach). Kein neues Feld im Format, Löschen anderer Körper ändert die Farbe nicht.
6. **Skizzen** erscheinen als Symbole in der Kette der Körper, die sie verwenden. Eine Rückrollstelle auf einer Skizze setzt deren Körper zurück; die Skizze bleibt im Baum unter „Skizzen“ sichtbar und bearbeitbar.
7. **Globaler Marker:** Er hat in der Oberfläche keine eigene Bedienung mehr. Liegt er vor Schritten des gewählten Körpers, zeigt die Leiste diese abgeblendet mit einem Hinweis „Gesamter Verlauf zurückgesetzt“ und einem Knopf „Alles aktivieren“. Ziehen bewegt immer die Stelle des gewählten Körpers.
8. **Formatversion:** die nächste freie Nummer; Speichern mit Rückrollstellen braucht keine zusätzliche Warnung (alte Programmstände lehnen die Datei mit der üblichen Versionsmeldung ab).

## Bewusste Entscheidungen beim Bau (Stand Aufgabe 8)

9. **Kantenkopien in eine offene Skizze** aus einem zurückgesetzten Körper sind **nicht** gesperrt. Das ist Absicht: Sie ändern den Körper nicht, und die Skizze verweist nur auf die Kante.
10. **Das Auge** gibt es nur in Körperzeilen. Ausblenden betrifft nur die Ansicht, gilt je Tab und wird nicht gespeichert.
11. **Feste Reihenfolge der Körperzeilen:** Die Körper stehen nach ihrem ersten Schritt (Wurzel der Kette) im Baum, damit ein Verschieben oder Bearbeiten nichts umsortiert.
12. **Kontextmenü der Schritte in der Leiste** (über den Entwurf hinaus, vom Bauherrn bestätigt): Unterdrücken, Umbenennen und Löschen einzelner Schritte gehen per Rechtsklick auf den Schritt in der Leiste. Die Körperzeilen im Baum wirken auf den Ursprung des Körpers.
13. **Die Sperre ist vorsichtig:** Eine Skizze, die mehrere Körper verwenden, und jeder Eingang eines Vereinigens sperren, sobald einer der beteiligten Körper zurückgesetzt ist. Das kann einmal mehr sperren als nötig, nie weniger. Der Hinweis nennt den Ausweg („Zum Ende“).
14. **Durchsichtiger Baum:** Im durchsichtigen Baum haben ruhende Zeilen eine kräftigere Schriftfarbe (etwa #d0d0d0 dunkel, #2a2a2e hell). Die Hinterlegung der Zeilen wird aus der tatsächlichen Schriftfarbe so berechnet, dass der Kontrast mindestens 4,5 : 1 bleibt. Bei 80 % Durchsichtigkeit bleiben etwa 0,68 (dunkel) und 0,66 (hell) Deckkraft in der Zeilenfläche.
15. **`history_body()`** ersetzt `selected_body` für die Leiste: `selected_body` war schon der Index in der Körperliste für die Eigenschaften; die Leiste braucht die Kennung der Spitze (`select_body` und `history_body()` in `AppState`).
