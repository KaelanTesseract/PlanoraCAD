#!/usr/bin/env python3
"""Wöchentlicher Bericht über die fremden Bibliotheken (siehe .github/workflows/deps-check.yml).

Prüft den Programm-Arbeitsbereich (Cargo.lock) und die Beispiel-Plug-ins (plugins/Cargo.lock):
Sicherheitsmeldungen und Lizenzen (cargo deny), verträgliche Updates (cargo update --dry-run),
neue Fassungen direkter Abhängigkeiten (cargo outdated) und die Wiedervorlagen aus deny.toml.

Schreibt Markdown nach stdout. Endet mit 1 genau dann, wenn cargo deny für einen der beiden
Arbeitsbereiche eine Sicherheitsmeldung (advisories) als Fehler meldet; sonst mit 0. Mit
`--issue DATEI` steht dieselbe Fassung, auf die Länge eines GitHub-Issues gekürzt, zusätzlich in
DATEI.

Aufruf im Wurzelordner des Repos: python3 .github/scripts/deps_report.py [--issue issue.md]
"""

import argparse
import datetime
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONFIG = ROOT / "deny.toml"
WORKSPACES = [("Programm", ROOT), ("Beispiel-Plug-ins", ROOT / "plugins")]
# GitHub erlaubt 65536 Zeichen im Issue-Text; etwas Luft für den Hinweis am Ende.
ISSUE_MAX = 60000


def run(args, cwd=ROOT):
    """Startet einen Befehl; liefert (Exit-Code, stdout, stderr). Fehlt das Programm: 127."""
    try:
        p = subprocess.run(args, cwd=cwd, capture_output=True, text=True, encoding="utf-8")
    except FileNotFoundError as e:
        return 127, "", str(e)
    return p.returncode, p.stdout, p.stderr


def deny(ws, which):
    """cargo deny für `which` (advisories oder licenses/bans/sources): (Code, Diagnosen)."""
    code, _, err = run(
        ["cargo", "deny", "--format", "json", "--manifest-path", str(ws / "Cargo.toml"),
         "--config", str(CONFIG), "check", *which]
    )
    diags = []
    for line in err.splitlines():
        try:
            o = json.loads(line)
        except json.JSONDecodeError:
            continue
        if o.get("type") == "diagnostic":
            diags.append(o.get("fields", {}))
    if code != 0 and not diags:
        diags.append({"severity": "error", "message": err.strip()[-500:] or "cargo deny scheiterte"})
    return code, diags


def crate_of(diag):
    """Paket und Fassung einer Diagnose (aus dem ersten Graphen), sonst leer."""
    for g in diag.get("graphs") or []:
        k = g.get("Krate") or {}
        if k.get("name"):
            return f"{k['name']} {k.get('version', '')}".strip()
    return ""


def compatible_updates(ws):
    """Verträgliche Updates laut `cargo update --dry-run`: Liste (Paket, alt, neu)."""
    code, _, err = run(["cargo", "update", "--dry-run"], cwd=ws)
    if code != 0:
        return None, err.strip()[-500:]
    ups = re.findall(r"Updating (\S+) v(\S+) -> v(\S+)", err)
    return ups, ""


def big_jump(old, new):
    """Großer Sprung: andere Hauptversion, bei 0.x andere Nebenversion."""
    a, b = old.lstrip("=^~").split("."), new.split(".")
    if not a or not b:
        return True
    if a[0] == "0" and b[0] == "0":
        return a[:2] != b[:2]
    return a[0] != b[0]


def outdated_direct(ws):
    """Neue Fassungen direkter Abhängigkeiten laut `cargo outdated`: Liste (Paket, jetzt, neu)."""
    code, out, err = run(
        ["cargo", "outdated", "--workspace", "--root-deps-only", "--format", "json"], cwd=ws
    )
    if code != 0:
        return None, err.strip()[-500:]
    found = {}
    for line in out.splitlines():
        try:
            o = json.loads(line)
        except json.JSONDecodeError:
            continue
        for d in o.get("dependencies", []):
            latest, project = d.get("latest", ""), d.get("project", "")
            if re.match(r"^\d", latest) and latest != project:
                found[d["name"]] = (d["name"], project, latest)
    return sorted(found.values()), ""


def latest_version(name):
    """Neueste veröffentlichte Fassung laut `cargo info`, sonst None."""
    code, out, _ = run(["cargo", "info", name])
    m = re.search(r"^version: (\S+)(?: \(latest (\S+)\))?", out, re.M)
    if code != 0 or not m:
        return None
    return m.group(2) or m.group(1)


def parents(crate):
    """Direkte Abhängige von `crate` im Programm (alle Zielsysteme): Liste (Paket, Fassung)."""
    code, out, _ = run(
        ["cargo", "tree", "-i", crate, "--target", "all", "-e", "normal", "--prefix", "depth",
         "--depth", "1"]
    )
    if code != 0:
        return []
    found = []
    for line in out.splitlines():
        m = re.match(r"^1(\S+) v(\S+)", line)
        if m and (m.group(1), m.group(2)) not in found:
            found.append((m.group(1), m.group(2)))
    return found


def section_security(lines, name, code, diags):
    lines.append(f"### {name}")
    errors = [d for d in diags if d.get("severity") == "error"]
    if code == 0 and not errors:
        lines.append("Keine Meldungen.")
    for d in errors:
        adv = d.get("advisory") or {}
        what = adv.get("id") or d.get("code") or ""
        title = adv.get("title") or d.get("message", "")
        kind = {"unmaintained": "nicht mehr gepflegt", "vulnerability": "Sicherheitslücke"}.get(
            d.get("code"), "Meldung"
        )
        lines.append(f"- **{what}** {crate_of(d)} ({kind}): {title}")
    lines.append("")


def section_rules(lines, name, code, diags):
    errors = [d for d in diags if d.get("severity") == "error"]
    if code == 0 and not errors:
        lines.append(f"- {name}: in Ordnung.")
        return
    lines.append(f"- {name}: **{len(errors)} Fehler**")
    for d in errors[:20]:
        lines.append(f"  - {d.get('code', '')} {crate_of(d)}: {d.get('message', '')}")


def report():
    lines = []
    today = datetime.date.today().isoformat()
    _, sha, _ = run(["git", "rev-parse", "--short", "HEAD"])
    lines.append(f"# Wöchentliche Prüfung der Bibliotheken ({today}, Stand {sha.strip()})")
    lines.append("")
    lines.append(
        "Nur ein Bericht: Hier wird nichts geändert. Sicherheitsmeldungen (auch „nicht mehr "
        "gepflegt“) und Prüffehler machen den Lauf rot."
    )
    lines.append("")

    failed = False
    adv = {}
    lines.append("## Sicherheit und Pflege (cargo deny advisories)")
    lines.append("")
    for name, ws in WORKSPACES:
        code, diags = deny(ws, ["advisories"])
        adv[name] = diags
        failed |= code != 0
        section_security(lines, name, code, diags)

    lines.append("## Lizenzen, Verbote, Quellen (cargo deny)")
    lines.append("")
    for name, ws in WORKSPACES:
        code, diags = deny(ws, ["licenses", "bans", "sources"])
        section_rules(lines, name, code, diags)
    lines.append("")

    lines.append("## Verträgliche Updates (mit `cargo update` erledigt)")
    lines.append("")
    for name, ws in WORKSPACES:
        ups, err = compatible_updates(ws)
        if ups is None:
            lines.append(f"- {name}: Prüfung scheiterte: `{err}`")
        elif not ups:
            lines.append(f"- {name}: alles auf dem neuesten verträglichen Stand.")
        else:
            lines.append(f"<details><summary>{name}: {len(ups)} Pakete</summary>")
            lines.append("")
            lines.extend(f"- {n} {a} → {b}" for n, a, b in ups)
            lines.append("")
            lines.append("</details>")
    lines.append("")

    lines.append("## Neue Fassungen direkter Abhängigkeiten (feste Fassung in Cargo.toml anheben)")
    lines.append("")
    for name, ws in WORKSPACES:
        found, err = outdated_direct(ws)
        lines.append(f"### {name}")
        if found is None:
            lines.append(f"Prüfung scheiterte: `{err}`")
        elif not found:
            lines.append("Alle direkten Abhängigkeiten sind aktuell.")
        else:
            big = [f for f in found if big_jump(f[1], f[2])]
            small = [f for f in found if not big_jump(f[1], f[2])]
            if small:
                lines.append("Kleiner Schritt:")
                lines.extend(f"- {n} {a} → {b}" for n, a, b in small)
            if big:
                lines.append("Großer Sprung (echte Arbeit, Änderungen lesen):")
                lines.extend(f"- {n} {a} → {b}" for n, a, b in big)
        lines.append("")

    lines.append("## Wiedervorlagen aus deny.toml")
    lines.append("")
    ignores = tomllib.loads(CONFIG.read_text(encoding="utf-8")).get("advisories", {}).get(
        "ignore", []
    )
    not_seen = {
        label.get("span")
        for d in adv.get("Programm", [])
        if d.get("code") == "advisory-not-detected"
        for label in d.get("labels") or []
    }
    if not ignores:
        lines.append("Keine Ausnahmen.")
    for entry in ignores:
        aid = entry.get("id") if isinstance(entry, dict) else entry
        reason = entry.get("reason", "") if isinstance(entry, dict) else ""
        crate = reason.split(":", 1)[0].strip() if ":" in reason else ""
        if aid in not_seen:
            lines.append(f"- **{aid}** ({crate or '?'}): im Programm nicht mehr gefunden, die "
                         "Ausnahme kann weg.")
            continue
        via = []
        for pname, pver in parents(crate) if crate else []:
            newest = latest_version(pname)
            note = f", neu: {newest}" if newest and newest != pver else ", aktuell"
            via.append(f"{pname} {pver}{note if newest else ''}")
        chain = "; ".join(via) if via else "Kette unbekannt"
        lines.append(f"- **{aid}** ({crate or '?'}) über {chain}")
    lines.append("")
    return "\n".join(lines), failed


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--issue", type=Path, help="gekürzte Fassung für das Issue hierhin schreiben")
    args = ap.parse_args()
    text, failed = report()
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stdout.write(text)
    if args.issue:
        short = text
        if len(short) > ISSUE_MAX:
            short = short[:ISSUE_MAX]
            # Ein angeschnittener aufklappbarer Block wird geschlossen, damit der Hinweis sichtbar ist.
            if short.count("<details>") > short.count("</details>"):
                short += "\n\n</details>"
            short += "\n\n… gekürzt; der ganze Bericht steht im Lauf.\n"
        args.issue.write_text(short, encoding="utf-8")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
