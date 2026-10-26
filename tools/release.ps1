<#
.SYNOPSIS
  Hilfe fuer Dennis beim Veroeffentlichen. Setzt KEINEN Tag, pusht nichts, loest nichts auf GitHub aus.

.DESCRIPTION
  Ablauf (siehe README, Abschnitt "Updates und Versionen"):

    1. Neue Version setzen:     tools\release.ps1 -NewVersion 0.2.0-dev.2
       (aendert workspace.package.version in Cargo.toml und Cargo.lock; danach CHANGELOG.md
       um einen Abschnitt "## 0.2.0-dev.2" ergaenzen, von Hand pruefen und committen)
    2. Pruefen:                 tools\release.ps1
       - Version in Cargo.toml gueltig, CHANGELOG.md hat den Abschnitt
       - Arbeitsverzeichnis sauber
       - optional mit -RunChecks: fmt, clippy, Tests, cargo deny lokal
       - auf GitHub gibt es einen erfolgreichen Lauf von ci.yml fuer genau diesen Commit,
         sonst Abbruch mit dem Befehl, der ihn startet
    3. Das Skript gibt die Befehle fuer Tag und Push nur aus. Ausfuehren tust du sie selbst.

  Laeuft unter Windows PowerShell 5.1 und PowerShell 7. Braucht cargo und git, fuer die
  CI-Pruefung die GitHub-Kommandozeile "gh" (angemeldet).

.EXAMPLE
  powershell -File tools\release.ps1 -NewVersion 0.2.0-dev.2
  powershell -File tools\release.ps1 -RunChecks
#>
param(
    [string]$NewVersion = '',
    [switch]$RunChecks
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$utf8 = New-Object System.Text.UTF8Encoding($false)
$versionPattern = '^(\d+)\.(\d+)\.(\d+)(?:-(dev|beta)\.(\d+))?$'

function Stop-Release([string]$message) {
    [Console]::Error.WriteLine('ABBRUCH: ' + $message)
    exit 1
}

# Rangfolge wie in planora-update: Dev < Beta < Stabil, sonst nach Zahlen.
function Get-VersionKey([string]$text) {
    $m = [regex]::Match($text, $versionPattern)
    if (-not $m.Success) { return $null }
    $kind = 2
    $num = 0
    if ($m.Groups[4].Success) {
        if ($m.Groups[4].Value -eq 'dev') { $kind = 0 } else { $kind = 1 }
        $num = [int]$m.Groups[5].Value
    }
    return ('{0:D6}.{1:D6}.{2:D6}.{3}.{4:D6}' -f [int]$m.Groups[1].Value, [int]$m.Groups[2].Value, [int]$m.Groups[3].Value, $kind, $num)
}

# Liest die Zeilen von Cargo.toml und sucht "version = ..." im Abschnitt [workspace.package].
# Gibt den Zeilenindex (oder -1) und die Zeilen zurueck.
function Find-CargoVersionLine([string[]]$lines) {
    $inSection = $false
    for ($i = 0; $i -lt $lines.Count; $i++) {
        $t = $lines[$i].Trim()
        if ($t.StartsWith('[')) { $inSection = ($t -eq '[workspace.package]'); continue }
        if ($inSection -and $t -match '^version\s*=\s*"[^"]+"') { return $i }
    }
    return -1
}

function Get-CargoVersion {
    $text = [System.IO.File]::ReadAllText((Join-Path $root 'Cargo.toml'), $utf8)
    $lines = $text -split "`n"
    $i = Find-CargoVersionLine $lines
    if ($i -lt 0) { Stop-Release 'workspace.package.version in Cargo.toml nicht gefunden.' }
    return [regex]::Match($lines[$i], '"([^"]+)"').Groups[1].Value
}

# ---- 1. Version erhoehen (nur auf Wunsch) ----
if ($NewVersion -ne '') {
    $NewVersion = $NewVersion.TrimStart('v')
    $newKey = Get-VersionKey $NewVersion
    if ($null -eq $newKey) { Stop-Release "'$NewVersion' ist keine erlaubte Version (erlaubt: 1.2.3, 1.2.3-beta.1, 1.2.3-dev.1)." }
    $oldVersion = Get-CargoVersion
    $oldKey = Get-VersionKey $oldVersion
    if ($null -eq $oldKey -or [string]::CompareOrdinal($newKey, $oldKey) -le 0) {
        Stop-Release "Die neue Version $NewVersion muss hoeher sein als die jetzige ($oldVersion)."
    }
    $path = Join-Path $root 'Cargo.toml'
    $toml = [System.IO.File]::ReadAllText($path, $utf8)
    $lines = $toml -split "`n"
    $i = Find-CargoVersionLine $lines
    if ($i -lt 0) { Stop-Release 'workspace.package.version in Cargo.toml nicht gefunden.' }
    $lines[$i] = ([regex]'"[^"]+"').Replace($lines[$i], ('"' + $NewVersion + '"'), 1)
    $toml = $lines -join "`n"
    [System.IO.File]::WriteAllText($path, $toml, $utf8)
    # Cargo.lock nachziehen (nur die Versionen der eigenen Crates, ohne Netz)
    & cargo metadata --format-version 1 --offline | Out-Null
    if ($LASTEXITCODE -ne 0) { Stop-Release 'Cargo.lock konnte nicht aktualisiert werden.' }
    & cargo metadata --format-version 1 --offline --locked | Out-Null
    if ($LASTEXITCODE -ne 0) { Stop-Release 'Cargo.lock passt nach der Aenderung nicht zu Cargo.toml.' }
    Write-Output "Version von $oldVersion auf $NewVersion gesetzt (Cargo.toml und Cargo.lock)."
    Write-Output ''
    Write-Output 'Als Naechstes:'
    Write-Output "  1. In CHANGELOG.md oben den Abschnitt '## $NewVersion' mit '### Neu', '### Verbessert', '### Behoben' schreiben (einfache Sprache)."
    Write-Output '  2. Aenderungen ansehen und committen.'
    Write-Output '  3. Dieses Skript ohne -NewVersion noch einmal starten.'
    exit 0
}

# ---- 2. Pruefungen ----
$version = Get-CargoVersion
if ($null -eq (Get-VersionKey $version)) { Stop-Release "Die Version '$version' in Cargo.toml ist keine erlaubte Version." }
Write-Output "Version in Cargo.toml: $version"
$tag = "v$version"

& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'changelog-section.ps1') -Version $version | Out-Null
if ($LASTEXITCODE -ne 0) { Stop-Release "CHANGELOG.md hat keinen (oder einen leeren) Abschnitt '## $version'." }
Write-Output 'CHANGELOG.md: Abschnitt vorhanden.'

& git rev-parse -q --verify "refs/tags/$tag" | Out-Null
if ($LASTEXITCODE -eq 0) { Stop-Release "Den Tag $tag gibt es lokal schon. Eine Version wird nur einmal veroeffentlicht; nimm eine hoehere Versionsnummer." }
$remoteTag = & git ls-remote --tags origin "refs/tags/$tag"
if ($LASTEXITCODE -ne 0) { Stop-Release 'GitHub (git ls-remote) ist nicht erreichbar; ob der Tag schon existiert, laesst sich nicht pruefen.' }
if ($remoteTag) { Stop-Release "Den Tag $tag gibt es auf GitHub schon. Eine Version wird nur einmal veroeffentlicht; nimm eine hoehere Versionsnummer." }
Write-Output "Tag ${tag}: noch frei."

# Cargo.lock muss zur Version passen (sonst bricht der Bau mit --locked auf GitHub ab).
& cargo metadata --format-version 1 --offline --locked | Out-Null
if ($LASTEXITCODE -ne 0) { Stop-Release 'Cargo.lock ist nicht aktuell (cargo metadata --locked schlaegt fehl). Mit "cargo metadata --offline" nachziehen und committen.' }
Write-Output 'Cargo.lock: aktuell.'

$dirty = & git status --porcelain
if ($dirty) { Stop-Release 'Es gibt uncommittete Aenderungen. Erst committen, dann veroeffentlichen.' }
Write-Output 'Arbeitsverzeichnis: sauber.'

$checks = @(
    'cargo fmt --all --check',
    'cargo clippy --workspace --all-targets -- -D warnings',
    'cargo test --workspace --no-fail-fast',
    'cargo deny check'
)
if ($RunChecks) {
    foreach ($c in $checks) {
        Write-Output "> $c"
        $parts = $c.Split(' ')
        & $parts[0] $parts[1..($parts.Count - 1)]
        if ($LASTEXITCODE -ne 0) { Stop-Release "Pruefung fehlgeschlagen: $c" }
    }
    Write-Output 'Lokale Pruefungen: alle gruen.'
} else {
    Write-Output 'Lokale Pruefungen (nicht gestartet; mit -RunChecks erledigt das Skript sie):'
    foreach ($c in $checks) { Write-Output "  $c" }
    Write-Output '  Unter Linux zusaetzlich: cargo test --workspace --no-fail-fast (z. B. in WSL als Benutzer ci).'
}

$head = (& git rev-parse HEAD).Trim()
$branch = (& git rev-parse --abbrev-ref HEAD).Trim()
Write-Output "Commit: $head (Zweig $branch)"

# ---- 3. Erfolgreicher ci.yml-Lauf fuer genau diesen Commit? ----
if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    Stop-Release 'Die GitHub-Kommandozeile "gh" fehlt, die CI-Pruefung ist nicht moeglich.'
}
$json = & gh run list --workflow ci.yml --event workflow_dispatch --limit 100 --json headSha,conclusion | Out-String
if ($LASTEXITCODE -ne 0) { Stop-Release 'gh run list ist fehlgeschlagen (angemeldet? "gh auth status").' }
$runs = @()
if ($json.Trim().Length -gt 0) { $runs = @($json | ConvertFrom-Json) }
$ok = $false
foreach ($r in $runs) { if ($r.headSha -eq $head -and $r.conclusion -eq 'success') { $ok = $true } }
if (-not $ok) {
    [Console]::Error.WriteLine('ABBRUCH: Fuer diesen Commit gibt es keinen erfolgreichen ci.yml-Lauf auf GitHub.')
    [Console]::Error.WriteLine('  1. Den Stand nach GitHub bringen:  git push origin ' + $branch)
    [Console]::Error.WriteLine('  2. Die volle Pruefung starten:     gh workflow run ci.yml --ref ' + $branch)
    [Console]::Error.WriteLine('  3. Warten:                         gh run watch   (muss gruen enden)')
    [Console]::Error.WriteLine('  4. Dieses Skript noch einmal starten.')
    exit 1
}
Write-Output 'GitHub-CI: erfolgreicher ci.yml-Lauf fuer diesen Commit gefunden.'

# ---- 4. Nur vorschlagen ----
Write-Output ''
Write-Output 'Alles bereit. Das Skript setzt keinen Tag. Zum Veroeffentlichen selbst ausfuehren:'
Write-Output "  git tag -a $tag -m `"PlanoraCAD $version`""
Write-Output "  git push origin $tag"
Write-Output 'Danach baut release.yml die Version und legt das Release an.'
exit 0
