# PlanoraCAD

Ein quelloffenes, parametrisches 3D-CAD in reinem Rust für Windows und Linux.

**Status:** frühe Entwicklung (Fundament, Kernel-Basis, Boolesche Operationen).

Erste Version: Quader, Zylinder, Kugel, Kegel, Extrusion und Rotation von Beispielprofilen, 3D-Ansicht mit Auswahl, Körper verschieben (Feld „Position“) sowie Boolesche Operationen (Vereinigen, Abziehen, Schneiden) für Körper mit Flächen aus Ebenen, Zylindern, Kegeln, Kugeln und Tori (Stufe A).

Bekannte Grenzen: Schnitte, die Raumkurven ergeben (z. B. zwei Zylinder mit gekreuzten Achsen), werden noch nicht unterstützt; das Vereinigen von Körpern, die sich nur an einer Kante berühren, und manche genau tangentialen Lagen liefern eine Fehlermeldung statt eines Ergebnisses (bei Berührung nur an einer Ecke entsteht ein Körper aus zwei Teilen mit gemeinsamer Ecke); es gibt noch kein Rückgängig. Details: `docs/superpowers/offene-punkte.md`.

Lizenz: GPL-3.0-only (siehe `LICENSE`).

## Bauen

    cargo run -p planora-app --release

Unter Linux werden zusätzlich die System-Bibliotheken aus `.github/workflows/ci.yml` benötigt.

## Entwicklung

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo deny check

## Drittanbieter
- Schrift: Roboto 2.138, Apache-2.0 (siehe `crates/planora-app/assets/fonts/`).
