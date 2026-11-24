# Programmfenster merken – Kurzentwurf

Stand 2026-11-24, freigegeben von Dennis („Ja, so bauen“).

**Warum:** Das Programm soll so öffnen, wie es geschlossen wurde (Größe, Lage, maximiert), wie Fusion.
Die Bildschirmauflösung selbst legt Windows fest; gemerkt wird das Fenster samt Skalierung des Monitors.

**Umsetzung:**
- Konfiguration: `main_window` (`MainWindow`: `WindowState` + `scale`), nachsichtig gelesen, ohne Wert nicht geschrieben.
- App verfolgt je Bild den Stand (`main_window::track`): minimiert → unverändert; maximiert → normaler Stand
  bleibt, nur „maximiert“ dazu; sonst gemeldeter Stand. Beim sauberen Schließen speichert nur das Programm
  (nicht die Test-App) das Feld (`main_window::save`).
- Start (`main_window::at_start`): nach einem Update das Fenster aus dem Update, sonst das gemerkte; liegt der
  Kopfstreifen (Punkte × Skalierung) auf keinem Monitor, fällt die Lage weg, die Größe bleibt.
- Nach Absturz oder Beenden von außen gilt der Stand vom letzten sauberen Schließen.

**Verworfen:** laufendes Mitspeichern (ständiges Schreiben der Konfiguration), eigener Speicher außerhalb der
Konfiguration.

## Nach Prüfer (2026-11-24)
- Die Lage geht nicht über den Fenster-Builder (winit rechnet sie mit der Skalierung des Monitors um, auf dem das
  neue Fenster entsteht; bei gemischter Skalierung öffnete es neben allen Monitoren). Die ersten Bilder schieben
  das Fenster auf die gemerkte Stelle in physischen Pixeln, umgerechnet mit der Skalierung des Monitors, auf dem
  es steht (höchstens drei Versuche), und maximieren es erst danach (`main_window::place_step`).
- Lagen links oder oberhalb des Hauptmonitors bleiben (`WindowState::sanitized_any_monitor`); ob sie auf einem
  Monitor liegen, prüft der Start.
- Der gemerkte Stand ist ab dem Start vorbelegt: Ein maximiert gestartetes und so geschlossenes Fenster behält
  seine normale Größe. Verfolgt wird erst nach dem Platzieren.
- Gemerktes Fenster vor dem aus einem Update (ältere Versionen ohne `main_window`: das aus dem Update).
- Ohne gemeldete Lage (Wayland) bleibt der gemerkte Stand; Wayland erlaubt Programmen keine Lage, gemerkt wird
  dort nichts Neues (Grenze).
- Grenze: Ein maximiertes Fenster, das per Tastatur auf einen anderen Monitor geschoben wurde, öffnet maximiert
  auf dem Monitor seines normalen Stands.
