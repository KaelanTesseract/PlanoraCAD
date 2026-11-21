#!/usr/bin/env bash
# Baut die Beispiel-Plug-ins (plugins/) und packt sie als .pcplugin nach
# crates/planora-plugin-host/tests/data/, dazu je Plug-in den Quell-Hash (<name>.quelle-sha256).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

rustup target add wasm32-wasip2

(cd plugins && cargo build --release --target wasm32-wasip2)

for name in beispiel-hallo test-faelle; do
    wasm="plugins/target/wasm32-wasip2/release/${name//-/_}.wasm"
    cargo run --quiet -p planora-plugin-host --example pack -- \
        "plugins/$name" "$wasm" crates/planora-plugin-host/tests/data
done
