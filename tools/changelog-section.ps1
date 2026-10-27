<#
.SYNOPSIS
  Gibt den Abschnitt einer Version aus CHANGELOG.md aus (ohne die Ueberschrift "## <Version>").

.DESCRIPTION
  Wird von release.yml (Beschreibung des Releases) und tools/release.ps1 (Pruefung) benutzt.
  Laeuft unter Windows PowerShell 5.1 und PowerShell 7.
  Endet mit Fehlercode 1, wenn es keinen Abschnitt fuer die Version gibt oder er leer ist.

.EXAMPLE
  powershell -File tools\changelog-section.ps1 -Version 0.2.0-dev.1
  powershell -File tools\changelog-section.ps1 -Version 0.2.0-dev.1 -OutFile notes.md

.EXAMPLE
  powershell -File tools\changelog-section.ps1 -Version 0.2.0-dev.1 -Footer
  Haengt eine Leerzeile und den Hinweis "So meldest du etwas: ..." an (fuer die Beschreibung des Releases).
#>
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$Path = '',
    [string]$OutFile = '',
    [switch]$Footer
)

$ErrorActionPreference = 'Stop'
$Version = $Version.TrimStart('v')
if ($Path -eq '') { $Path = Join-Path (Split-Path -Parent $PSScriptRoot) 'CHANGELOG.md' }
$utf8 = New-Object System.Text.UTF8Encoding($false)

if (-not (Test-Path -LiteralPath $Path)) {
    [Console]::Error.WriteLine("CHANGELOG.md nicht gefunden: $Path")
    exit 1
}

$text = [System.IO.File]::ReadAllText($Path, $utf8)
$lines = $text -split "`r?`n"

$collected = New-Object System.Collections.Generic.List[string]
$inSection = $false
$found = $false
foreach ($line in $lines) {
    if ($line.StartsWith('## ')) {
        if ($inSection) { break }
        if ($line.Substring(3).Trim() -ceq $Version) {
            $inSection = $true
            $found = $true
            continue
        }
    }
    if ($inSection) { $collected.Add($line) }
}

if (-not $found) {
    [Console]::Error.WriteLine("In CHANGELOG.md fehlt der Abschnitt '## $Version'.")
    exit 1
}

$body = ($collected -join "`n").Trim()
if ($body.Length -eq 0) {
    [Console]::Error.WriteLine("Der Abschnitt '## $Version' in CHANGELOG.md ist leer.")
    exit 1
}

if ($Footer) {
    $body = $body + "`n`nSo meldest du etwas: https://github.com/KaelanTesseract/PlanoraCAD/blob/master/docs/so-meldest-du-etwas.md"
}

if ($OutFile -ne '') {
    [System.IO.File]::WriteAllText($OutFile, $body + "`n", $utf8)
} else {
    [Console]::OutputEncoding = $utf8
    Write-Output $body
}
