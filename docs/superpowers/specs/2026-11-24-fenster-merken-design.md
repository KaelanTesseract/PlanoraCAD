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
