# Baut die Beispiel-Plug-ins (plugins/) und packt sie als .pcplugin nach
# crates/planora-plugin-host/tests/data/, dazu je Plug-in den Quell-Hash (<name>.quelle-sha256).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

rustup target add wasm32-wasip2
if ($LASTEXITCODE -ne 0) { throw 'rustup target add wasm32-wasip2 ist fehlgeschlagen' }

Push-Location plugins
try {
    cargo build --release --target wasm32-wasip2
    if ($LASTEXITCODE -ne 0) { throw 'Bauen der Plug-ins ist fehlgeschlagen' }
} finally {
    Pop-Location
}

foreach ($name in 'beispiel-hallo', 'test-faelle') {
    $wasm = "plugins/target/wasm32-wasip2/release/$($name -replace '-', '_').wasm"
    cargo run --quiet -p planora-plugin-host --example pack -- "plugins/$name" $wasm crates/planora-plugin-host/tests/data
    if ($LASTEXITCODE -ne 0) { throw "Packen von $name ist fehlgeschlagen" }
}
