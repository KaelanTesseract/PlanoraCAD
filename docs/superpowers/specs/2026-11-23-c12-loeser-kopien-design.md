# C12: Löser ohne Kopie der Skizze je Bild (Kurzentwurf)

Freigegeben von Dennis am 2026-11-23 („Ja, so bauen“).

## Warum

Offener Punkt (Nachprüfung 2026-11-22): Kurvenzug (`curve_drag_to`), `solve_holding` und
`solve_holding_connected` sowie Status und Freiheitsgrade (`sk.clone().solve()`) kopieren die ganze
Skizze, beim Ziehen je Bild. Bei großen Skizzen kostet das spürbar Zeit. Das Verhalten soll gleich
bleiben, nur schneller.

## Ziele und Nicht-Ziele

- Ziel: gleiche Ergebnisse (Werte bitgleich, gleiche Ablehnungen), weniger Kopien je Bild.
- Nicht-Ziel: Umbau der Skizze auf billig kopierbare Daten (`Arc` je Arena). Großer Umbau ohne
  Gewinn beim Verhalten; verworfen.
- Nicht-Ziel: Alles lassen; der Punkt ist offen und große Skizzen ruckeln.

## Entwurf

1. **Messen zuerst:** Messtest mit großer Skizze (Rechtecke mit Muster), Zeit je Bild von
   `solve_holding` und einem Kurvenzug alt gegen neu.
2. **Wertestand `ValueSnapshot`** (planora-sketch): Werte, Stand, Strukturstand,
   Lösungsspeicher und Gruppen der Schleifenprüfung. `restore_values` setzt sie zurück; im
   Debug-Lauf prüft es, dass sich der Aufbau nicht geändert hat.
3. **`solve_holding` an Ort und Stelle:** Festpunkte direkt anhängen, danach mit
   `append_mark`/`cut_back` abschneiden; abgeschaltete Zeilen wieder an, Toleranz zurück,
   Wertestand zurück. Bei Erfolg bleiben die neuen Werte mit neuem Stand (`touch_values`), sonst
   ist die Skizze wie vorher (gleicher Stand).
4. **`solve_holding_connected`:** fremde Abhängigkeiten an Ort und Stelle aus- und wieder
   einschalten statt Kopie.
5. **Kurvenzug:** Beim gewöhnlichen Zug (ohne Text-Drehung, Radius, Rho, Mitnehmen und
   Krümmungsbogen-Suche) sichert er nur den Wertestand; die anderen Wege ändern mehr als Werte
   und behalten die volle Kopie.
6. **Status und Freiheitsgrade:** `dof_of` nimmt den Bericht aus dem Zwischenspeicher der Sitzung,
   wenn er zum Stand passt; sonst wie bisher. (`status_cached` ist schon je Stand gespeichert.)

## Prüfung

Vergleichstests alter gegen neuer Weg (Erfolg, Scheitern, Ablehnung beim Anheften), Daten
(`to_data`) nach Scheitern unverändert, Stand nach Scheitern gleich, Messtest. Volle Läufe
Windows und WSL; Handtest nur nach Go.

## Dem Bauenden überlassen

Namen der Hilfen, genaue Größe der Messskizze und Schwelle des Messtests (mit Begründung).
