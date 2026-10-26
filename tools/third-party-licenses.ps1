<#
.SYNOPSIS
  Erzeugt THIRD-PARTY-LICENSES.txt: jede Bibliothek, die in die Windows-Version einfliesst,
  mit Name, Version, Lizenz und Lizenztext, dazu die Schriften aus crates/planora-app/assets/fonts.

.DESCRIPTION
  Quelle ist "cargo metadata --filter-platform <Ziel>"; aufgenommen wird, was von den
  Programmteilen aus ueber normale und Build-Abhaengigkeiten erreichbar ist (reine
  Test-Abhaengigkeiten nicht). Die eigenen Crates (GPL-3.0-only) stehen nur im Kopf.
  Ausgabe: UTF-8 ohne BOM, Zeilenende LF, sortiert nach Name und Version, immer gleiches Format.
  Laeuft unter Windows PowerShell 5.1 und PowerShell 7. Cargo muss im Pfad sein.

.EXAMPLE
  powershell -File tools\third-party-licenses.ps1
  powershell -File tools\third-party-licenses.ps1 -Output out\THIRD-PARTY-LICENSES.txt
#>
param(
    [string]$Output = '',
    [string]$Target = 'x86_64-pc-windows-msvc'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if ($Output -eq '') { $Output = Join-Path $root 'THIRD-PARTY-LICENSES.txt' }
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Read-Normalized([string]$file) {
    $t = [System.IO.File]::ReadAllText($file, $utf8)
    $t = $t.TrimStart([char]0xFEFF)
    return (($t -replace "`r`n", "`n") -replace "`r", "`n").Trim()
}

Push-Location $root
try {
    $raw = & cargo metadata --format-version 1 --locked --filter-platform $Target | Out-String
    if ($LASTEXITCODE -ne 0) { throw "cargo metadata ist fehlgeschlagen (Code $LASTEXITCODE)." }
} finally {
    Pop-Location
}
$meta = $raw | ConvertFrom-Json

$byId = @{}
foreach ($p in $meta.packages) { $byId[$p.id] = $p }
$nodes = @{}
foreach ($n in $meta.resolve.nodes) { $nodes[$n.id] = $n }

# Alles, was von den Programmteilen aus ueber normale oder Build-Abhaengigkeiten erreichbar ist.
$seen = New-Object 'System.Collections.Generic.HashSet[string]'
$queue = New-Object 'System.Collections.Generic.Queue[string]'
foreach ($id in $meta.workspace_members) { if ($seen.Add($id)) { $queue.Enqueue($id) } }
while ($queue.Count -gt 0) {
    $id = $queue.Dequeue()
    $node = $nodes[$id]
    if ($null -eq $node) { continue }
    foreach ($d in $node.deps) {
        $used = $false
        foreach ($k in $d.dep_kinds) { if ($k.kind -ne 'dev') { $used = $true } }
        if ($used -and $seen.Add($d.pkg)) { $queue.Enqueue($d.pkg) }
    }
}

$libs = @()
foreach ($id in $seen) {
    $p = $byId[$id]
    if ($null -ne $p.source) { $libs += $p }   # eigene Crates (Pfad) haben keine Quelle
}
$libs = $libs | Sort-Object -Property @{Expression = { $_.name.ToLowerInvariant() } }, @{Expression = { $_.version } }

$licenseNames = '^(LICEN[SC]E|COPYING|UNLICENSE|NOTICE|COPYRIGHT)'
$sep = ('=' * 78)
$thin = ('-' * 78)
$sb = New-Object System.Text.StringBuilder
function Add-Line([string]$s) { [void]$sb.Append($s).Append("`n") }

$ws = $byId[$meta.workspace_members[0]]
Add-Line 'PlanoraCAD - Lizenzen von Drittanbietern'
Add-Line $sep
Add-Line ''
Add-Line 'PlanoraCAD selbst steht unter der GNU General Public License Version 3 (GPL-3.0-only).'
Add-Line "Version des Programms: $($ws.version)"
Add-Line ''
Add-Line "Diese Datei nennt alle Bibliotheken, die in die Windows-Version ($Target) einfliessen,"
Add-Line 'und alle mitgelieferten Schriften, jeweils mit Version, Lizenz und Lizenztext.'
Add-Line "Anzahl der Bibliotheken: $($libs.Count)"
Add-Line ''
Add-Line 'Inhalt'
Add-Line '  1. Schriften'
Add-Line '  2. Bibliotheken (alphabetisch)'
Add-Line ''
Add-Line $sep
Add-Line '1. Schriften'
Add-Line $sep

$fontDir = Join-Path $root 'crates/planora-app/assets/fonts'
$fonts = @(
    @{ Name = 'Roboto'; Version = '2.138'; License = 'Apache-2.0'; Files = 'Roboto-Regular.ttf'; Text = 'LICENSE-Roboto-Apache-2.0.txt' },
    @{ Name = 'Montserrat'; Version = '9.000'; License = 'OFL-1.1'; Files = 'Montserrat-Regular.ttf, Montserrat-Medium.ttf, Montserrat-SemiBold.ttf'; Text = 'OFL-Montserrat.txt' },
    @{ Name = 'Phosphor Icons (Light)'; Version = '2.1'; License = 'MIT'; Files = 'Phosphor-Light.ttf'; Text = 'LICENSE-Phosphor-MIT.txt' }
)
foreach ($f in $fonts) {
    Add-Line ''
    Add-Line $thin
    Add-Line "$($f.Name) $($f.Version)"
    Add-Line "Lizenz: $($f.License)"
    Add-Line "Dateien: $($f.Files)"
    Add-Line $thin
    Add-Line ''
    Add-Line (Read-Normalized (Join-Path $fontDir $f.Text))
}

Add-Line ''
Add-Line $sep
Add-Line '2. Bibliotheken'
Add-Line $sep

$missing = @()
foreach ($p in $libs) {
    $license = $p.license
    if ([string]::IsNullOrEmpty($license)) {
        if ($p.license_file) { $license = "siehe Lizenzdatei ($($p.license_file))" } else { $license = 'unbekannt' }
    }
    $dir = Split-Path -Parent $p.manifest_path

    $files = @()
    foreach ($item in (Get-ChildItem -LiteralPath $dir -File -ErrorAction SilentlyContinue)) {
        if ($item.Name -match $licenseNames) { $files += $item }
    }
    if ($p.license_file) {
        $lf = Join-Path $dir $p.license_file
        if (Test-Path -LiteralPath $lf -PathType Leaf) {
            $lfItem = Get-Item -LiteralPath $lf
            $already = $false
            foreach ($x in $files) { if ($x.FullName -eq $lfItem.FullName) { $already = $true } }
            if (-not $already) { $files += $lfItem }
        }
    }
    $files = @($files | Sort-Object -Property Name)

    Add-Line ''
    Add-Line $thin
    Add-Line "$($p.name) $($p.version)"
    Add-Line "Lizenz: $license"
    if ($p.repository) { Add-Line "Quelle: $($p.repository)" }
    Add-Line $thin

    $texts = New-Object 'System.Collections.Generic.HashSet[string]'
    $count = 0
    foreach ($f in $files) {
        $t = Read-Normalized $f.FullName
        if ($t.Length -eq 0 -or -not $texts.Add($t)) { continue }
        $count++
        Add-Line ''
        Add-Line "[$($f.Name)]"
        Add-Line $t
    }
    if ($count -eq 0) {
        # Das Paket bringt keine Lizenzdatei mit: Standardtext der angegebenen Lizenz(en) einsetzen.
        $missing += "$($p.name) $($p.version)"
        $ids = @()
        foreach ($m in [regex]::Matches([string]$p.license, '[A-Za-z0-9][A-Za-z0-9.+\-]*')) {
            $v = $m.Value
            if ($v -in @('OR', 'AND', 'WITH') -or $v -like '*-exception') { continue }
            if ($ids -notcontains $v) { $ids += $v }
        }
        if ($ids.Count -eq 0) { throw "Keine Lizenzdatei und keine Lizenzangabe fuer $($p.name) $($p.version)." }
        foreach ($id in $ids) {
            $std = Join-Path $PSScriptRoot "license-texts/$id.txt"
            if (-not (Test-Path -LiteralPath $std)) {
                throw "Kein Lizenztext fuer $($p.name) $($p.version): '$id' fehlt in tools/license-texts."
            }
            $holder = ($p.authors -join ', ')
            if ([string]::IsNullOrWhiteSpace($holder)) { $holder = "die Autoren von $($p.name)" }
            $t = (Read-Normalized $std).Replace('{{HOLDER}}', $holder)
            Add-Line ''
            Add-Line "[$id - Standardtext; das Paket enthaelt keine eigene Lizenzdatei]"
            Add-Line $t
        }
    }
}

if ($missing.Count -gt 0) {
    [Console]::Error.WriteLine('Hinweis: Standardtext eingesetzt (Paket ohne eigene Lizenzdatei): ' + ($missing -join ', '))
}

$dirOut = Split-Path -Parent $Output
if ($dirOut -and -not (Test-Path -LiteralPath $dirOut)) { New-Item -ItemType Directory -Path $dirOut | Out-Null }
[System.IO.File]::WriteAllText($Output, $sb.ToString(), $utf8)
Write-Output "Geschrieben: $Output ($($libs.Count) Bibliotheken, 3 Schriften)"
