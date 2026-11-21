# Wöchentliche Prüfung der Bibliotheken – Umsetzungsplan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ein wöchentlicher GitHub-Lauf berichtet über unsichere und veraltete Bibliotheken; vorher werden die offenen Abhängigkeitspunkte (chrono, async-executor) und der alte Testfehler `real_step_text_limit` erledigt.

**Architecture:** Ein eigener Workflow `deps-check.yml` installiert `cargo-deny` und `cargo-outdated` in fester Fassung, ruft das Berichtsskript `.github/scripts/deps-report.sh` auf und schreibt dessen Markdown in die Job Summary und in ein festes Issue. planora-app nutzt danach jiff statt chrono.

**Tech Stack:** GitHub Actions (ubuntu-latest), bash, `gh` CLI, cargo-deny 0.22.0, cargo-outdated 0.19.0, jiff 1.0.2.

**Spec:** `docs/superpowers/specs/2026-11-21-bibliotheken-pruefung-design.md`

## Global Constraints

- Zeitplan: montags 6 Uhr UTC (`cron: "0 6 * * 1"`) plus `workflow_dispatch`.
- Der Workflow baut und testet nichts; die übrigen CI-Läufe bleiben „nur vor einem Release“.
- Sicherheitsmeldung (deny advisories) → Lauf rot; nur Veraltetes → Lauf grün.
- Issue-Titel genau: `Wöchentliche Prüfung der Bibliotheken`; Text wird jede Woche ersetzt.
- Rechte: `contents: read`, `issues: write`.
- Neue Abhängigkeiten mit `=`-Fassung wie im übrigen `Cargo.toml`; Lizenz muss in `deny.toml` erlaubt sein.
- Kommentare deutsch; Texte der App nur über `tr!` (hier keine neuen App-Texte).
- Alte Dateien bytegleich: Absturzberichte, Rückmeldungs-Ordner und ZIP-Zeiten behalten exakt ihr Format.

## Review Focus

- Rückmeldungs-Ordner mit altem Namen (`2026-10-26_14-03-07…`) werden nach jiff weiter erkannt und nach `FEEDBACK_KEEP_DAYS` gelöscht → bestehende Tests in `tests/feedback.rs` bleiben unverändert grün.
- Absturzbericht-Dateiname und Zeitformat (`REPORT_TIME_FORMAT`) sind bytegleich → bestehende Tests `tests/crash.rs`, `crash_startup.rs`, `reports_english.rs` unverändert (nur Typ-Konstruktion angepasst).
- Lokale Zeit über Sommerzeitwechsel: `local_time.rs` liefert denselben Versatz wie bisher → bestehende Tests von `local_time` und `project_tiles.rs` grün.
- `cargo outdated` scheitert (Netz, Auflösung) → Bericht nennt den Fehler, Lauf wird nicht rot nur deswegen.
- Issue existiert noch nicht → Lauf legt es an; existiert es → wird ersetzt, kein zweites.

---

### Task 1: async-executor heben

**Files:** Modify: `Cargo.lock`

- [ ] `cargo update -p async-executor --precise 1.14.1`
- [ ] `cargo deny check` → `advisories ok, bans ok, licenses ok, sources ok`; `cargo build -p planora-app` baut.
- [ ] Commit „async-executor 1.14.1 (1.14.0 zurückgezogen)“.

### Task 2: planora-app von chrono auf jiff

**Files:**
- Modify: `Cargo.toml` (Workspace: `chrono` raus, `jiff = "=1.0.2"` rein), `crates/planora-app/Cargo.toml`
- Modify: `crates/planora-app/src/{crash.rs, crash_dialog.rs, diag.rs, feedback.rs, feedback_dialog.rs, hard_crash.rs, local_time.rs, zip_store.rs}`
- Modify Tests: `crates/planora-app/tests/{crash.rs, crash_startup.rs, diag.rs, diag_steps.rs, feedback.rs, project_tiles.rs, reports_english.rs}`

**Interfaces:**
- `crash::Report::time: jiff::civil::DateTime` (statt `chrono::NaiveDateTime`)
- `crash::report_file_name(t: jiff::civil::DateTime) -> String`
- `feedback::bundle_dir(data_dir: &Path, now: jiff::civil::DateTime) -> PathBuf`
- `feedback::cleanup_feedback_dirs(base: &Path, now: jiff::civil::DateTime)`
- Lokale Jetzt-Zeit: `jiff::Zoned::now().datetime()`; UTC: `jiff::Timestamp::now()`.

- [ ] Abhängigkeit tauschen; Format-Strings (`%Y-%m-%d_%H-%M-%S`, `REPORT_TIME_FORMAT`, `%d.%m.%Y, %H:%M:%S`, `%Y-%m-%dT%H:%M:%SZ`, `%H:%M:%S`) bleiben wörtlich (jiff `strftime`/`strptime`).
- [ ] `chrono::Duration::days(n)` → Vergleich über `jiff::Span::new().days(n)` bzw. `SignedDuration`; Verhalten „älter als `FEEDBACK_KEEP_DAYS` Tage“ unverändert.
- [ ] `local_time.rs` und `zip_store.rs` (DOS-Zeit aus `SystemTime`) mit `jiff::Timestamp::try_from(SystemTime)` und `.to_zoned(TimeZone::system())`.
- [ ] Tests nur in der Konstruktion umstellen (`jiff::civil::date(2026, 10, 26).at(h, m, s, 0)`), Erwartungen nicht ändern.
- [ ] `cargo test -p planora-app` grün; `cargo tree -i chrono` zeigt chrono nur noch unter `dxf`.
- [ ] `deny.toml`: Grund bei RUSTSEC-2026-0543 auf „nur noch über dxf“ einengen.
- [ ] Commit.

### Task 3: real_step_text_limit

**Files:** Modify: `crates/planora-project/tests/step_format.rs` (`import_and_save_at_limit`, ab Zeile 936)

Ursache: Die Hilfsfunktion prüft fest `"(0.1 MB, allowed 0.1 MB)"`; das stimmt nur für die kleine Grenze (64 KiB), nicht für 200 MiB.

- [ ] Erwartung aus `limit` ableiten: bei `limit < 1 MiB` die bisherige Zeile, sonst `en.contains(", allowed 200 MB)")` (bzw. die aus `limit` berechnete MB-Zahl).
- [ ] `cargo test -p planora-project --release --test step_format -- --ignored real_step_text_limit` → PASS; kleiner Test `import_refuses_more_step_text_than_can_be_saved` weiter PASS.
- [ ] Commit; in `CLAUDE.md`/Vault den Hinweis „scheitert bekannt“ entfernen.

### Task 4: Workflow und Bericht

**Files:**
- Create: `.github/workflows/deps-check.yml`, `.github/scripts/deps-report.sh`

**Interfaces:**
- `deps-report.sh` schreibt Markdown nach stdout und endet mit Code 1 genau dann, wenn `cargo deny check advisories` für einen der beiden Lockfiles scheitert; sonst 0.
- Abschnitte: „Sicherheit“, „Lizenzen und Regeln“, „Verträgliche Updates“, „Große Sprünge“, „Wiedervorlagen aus deny.toml“ – je für App und Plug-ins.
- Wiedervorlagen: für jede `ignore`-ID aus `deny.toml` die Kette (`cargo tree -i <crate> -e normal --depth 3`) und ob das ziehende Paket in `cargo outdated` auftaucht.

- [ ] Skript lokal in WSL gegen den Worktree laufen lassen; Bericht lesen, Exit-Code 0.
- [ ] Workflow: Werkzeuge mit `cargo install --locked cargo-deny@0.22.0 cargo-outdated@0.19.0`; Bericht in `$GITHUB_STEP_SUMMARY`; Issue per `gh issue list --search 'in:title "Wöchentliche Prüfung der Bibliotheken"'` finden, sonst `gh issue create`, sonst `gh issue edit --body-file`; zuletzt mit dem Exit-Code des Skripts enden.
- [ ] Prüfen: `actionlint`, falls verfügbar, sonst YAML-Prüfung per Python.
- [ ] Commit.

### Task 5: Doku und Abschluss

**Files:** Modify: `docs/superpowers/offene-punkte.md`, `CHANGELOG.md` (Abschnitt „Unveröffentlicht“, falls vorhanden), Projekt-`CLAUDE.md` (nicht im Git)

- [ ] `CLAUDE.md`: Ausnahme „deps-check läuft wöchentlich auf GitHub, baut nichts“; `real_step_text_limit`-Hinweis raus.
- [ ] Volle Prüfung Windows (fmt je Crate, clippy, alle Tests, deny) und WSL (`~/wsl-s6-full.sh`), danach WSL aufräumen.
- [ ] Erster echter Lauf erst nach Merge und Push (der Zeitplan gilt nur auf dem Standardzweig); `workflow_dispatch` dann einmal von Hand.
- [ ] Vault-Notiz.
