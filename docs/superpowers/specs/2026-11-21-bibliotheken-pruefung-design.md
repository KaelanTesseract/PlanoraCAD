# Wöchentliche Prüfung der Bibliotheken – Entwurf

Stand 2026-11-21, von Dennis freigegeben („passt so“).

## Warum

Alle fremden Bibliotheken sollen regelmäßig geprüft werden, damit PlanoraCAD immer die neueste und
sicherste Fassung nutzt. Heute prüft nur `cargo deny` (Sicherheitsmeldungen, Lizenzen), und das nur von
Hand und im Release-Lauf. Ob es neuere Fassungen gibt, prüft niemand.

## Ziele, Nicht-Ziele

- Ziel: Einmal pro Woche ein Bericht, was unsicher oder veraltet ist. Dennis entscheidet dann.
- Nicht-Ziel: selbstständiges Aktualisieren, Pull-Requests (kein Dependabot).
- Anti-Ziel: Die Prüfung darf die Regel „CI nur vor einem Release“ nicht aufweichen. Sie baut und testet
  nichts; sie ist die einzige, bewusste Ausnahme.

## Wege (abgewogen)

1. Prüfskript im Repo plus geplanter Claude-Lauf auf Dennis' Rechner: Einordnung in einfacher Sprache,
   braucht aber den laufenden Rechner.
2. Nur Skript über die Windows-Aufgabenplanung: ohne Einordnung und ohne Meldung.
3. **Zeitplan auf GitHub (gewählt von Dennis):** läuft auch bei ausgeschaltetem Rechner, Bericht auf
   GitHub, E-Mail bei Sicherheitsmeldungen.

## Entwurf

- Workflow `.github/workflows/deps-check.yml`: montags 6 Uhr (UTC-Zeitplan, etwa 7/8 Uhr deutscher Zeit)
  und von Hand startbar (`workflow_dispatch`). Nur Linux, baut nichts.
- Geprüft werden `Cargo.lock` und `plugins/Cargo.lock`:
  1. `cargo deny check advisories` (Sicherheit) und `cargo deny check` (alles, Lizenzen).
  2. `cargo outdated` getrennt nach „verträglich“ (mit `cargo update` erledigt) und „großer Sprung“.
  3. Wiedervorlagen aus `deny.toml`: Für jede erlaubte Ausnahme wird angezeigt, über welche Kette sie
     gezogen wird und ob es eine neuere Fassung des ziehenden Pakets gibt.
- Bericht: im Lauf selbst (Job Summary) und in einem festen Issue „Wöchentliche Prüfung der
  Bibliotheken“, dessen Text jede Woche ersetzt wird.
- Sicherheitsmeldung (auch „nicht mehr gepflegt“, wie cargo deny sie wertet) oder Prüffehler → Lauf rot → GitHub schickt E-Mail. Nur Veraltetes → Lauf grün.
- Issue: Das offene wird ersetzt; hat Dennis es geschlossen, legt der nächste Lauf ein neues an.
- Das Repo ist öffentlich, also auch das Issue; es nennt nur öffentlich bekannte Meldungen.
- GitHub schaltet Zeitpläne nach 60 Tagen ohne Push ab.

## Vorher (eigener Zweig)

- planora-app von chrono auf jiff (der DXF-Weg zieht chrono weiter, bis dxf es ablegt).
- `async-executor` 1.14.0 (zurückgezogen) → 1.14.1.
- `real_step_text_limit` untersuchen (alter Testfehler, keine Bibliothek).

## Dem Bauenden überlassen

Form des Berichts, Werkzeugfassungen (`cargo-deny`, `cargo-outdated` fest versioniert installieren),
Rechte des Workflows (`issues: write`, `contents: read`).
