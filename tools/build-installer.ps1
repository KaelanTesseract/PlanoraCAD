<#
.SYNOPSIS
  Baut den Installer planoracad-setup-x64.exe mit Inno Setup 6 und schreibt die Pruefsumme dazu.

.DESCRIPTION
  Vorgaben: Programmdatei target\release\planora-app.exe, Lizenzdatei dist\THIRD-PARTY-LICENSES.txt
  (fehlt sie, wird sie mit tools\third-party-licenses.ps1 erzeugt), Ausgabeordner dist.
  Die Version kommt aus [workspace.package] in Cargo.toml.
  Ergebnis: <OutDir>\planoracad-setup-x64.exe und <OutDir>\planoracad-setup-x64.sha256
  (Inhalt "<64 hex>  planoracad-setup-x64.exe", UTF-8 ohne BOM, LF).
  Installiert wird hier nichts. Laeuft unter Windows PowerShell 5.1 und PowerShell 7.

.EXAMPLE
  cargo build --release --locked -p planora-app
  powershell -File tools\build-installer.ps1
#>
param(
    [string]$Exe = '',
    [string]$Licenses = '',
    [string]$OutDir = '',
    [string]$Iscc = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$utf8 = New-Object System.Text.UTF8Encoding($false)
$setupName = 'planoracad-setup-x64.exe'
$shaName = 'planoracad-setup-x64.sha256'

function Stop-Build([string]$message) {
    [Console]::Error.WriteLine('ABBRUCH: ' + $message)
    exit 1
}

function Resolve-Full([string]$path) {
    if ([System.IO.Path]::IsPathRooted($path)) { return [System.IO.Path]::GetFullPath($path) }
    return [System.IO.Path]::GetFullPath((Join-Path $root $path))
}

if ($Exe -eq '') { $Exe = 'target\release\planora-app.exe' }
if ($Licenses -eq '') { $Licenses = 'dist\THIRD-PARTY-LICENSES.txt' }
if ($OutDir -eq '') { $OutDir = 'dist' }
$Exe = Resolve-Full $Exe
$Licenses = Resolve-Full $Licenses
$OutDir = Resolve-Full $OutDir

# ---- Version aus Cargo.toml ([workspace.package]) ----
$version = $null
$inSection = $false
foreach ($line in ([System.IO.File]::ReadAllText((Join-Path $root 'Cargo.toml'), $utf8) -split "`n")) {
    $t = $line.Trim()
    if ($t.StartsWith('[')) { $inSection = ($t -eq '[workspace.package]'); continue }
    if ($inSection -and $t -match '^version\s*=\s*"([^"]+)"') { $version = $Matches[1]; break }
}
if (-not $version) { Stop-Build 'workspace.package.version in Cargo.toml nicht gefunden.' }
$m = [regex]::Match($version, '^(\d+)\.(\d+)\.(\d+)')
if (-not $m.Success) { Stop-Build "Version '$version' hat keine drei Zahlen." }
$versionNumeric = '{0}.{1}.{2}' -f $m.Groups[1].Value, $m.Groups[2].Value, $m.Groups[3].Value

# ---- Eingaben pruefen ----
if (-not (Test-Path -LiteralPath $Exe)) {
    Stop-Build "Programmdatei fehlt: $Exe`nErst bauen: cargo build --release --locked -p planora-app"
}

$isccPath = $null
if ($Iscc -ne '') {
    if (Test-Path -LiteralPath $Iscc) { $isccPath = $Iscc }
}
else {
    $candidates = @()
    if ($env:LOCALAPPDATA) { $candidates += (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe') }
    if (${env:ProgramFiles(x86)}) { $candidates += (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe') }
    if ($env:ProgramFiles) { $candidates += (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe') }
    foreach ($c in $candidates) {
        if (Test-Path -LiteralPath $c) { $isccPath = $c; break }
    }
    if (-not $isccPath) {
        $cmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue
        if ($cmd) { $isccPath = $cmd.Source }
    }
}
if (-not $isccPath) {
    Stop-Build 'ISCC.exe (Inno Setup 6) nicht gefunden. Inno Setup 6 installieren (https://jrsoftware.org/isdl.php) oder -Iscc <Pfad> angeben.'
}

if (-not (Test-Path -LiteralPath $Licenses)) {
    $dir = Split-Path -Parent $Licenses
    if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
    & (Join-Path $PSScriptRoot 'third-party-licenses.ps1') -Output $Licenses
    if (-not (Test-Path -LiteralPath $Licenses)) { Stop-Build "Lizenzdatei konnte nicht erzeugt werden: $Licenses" }
}

if (-not (Test-Path -LiteralPath $OutDir)) { New-Item -ItemType Directory -Path $OutDir | Out-Null }

# ---- Installer bauen ----
$script = Join-Path $root 'installer\planoracad.iss'
& $isccPath /Qp "/DAppVersion=$version" "/DAppVersionNumeric=$versionNumeric" "/DSourceExe=$Exe" "/DLicensesFile=$Licenses" "/O$OutDir" $script
if ($LASTEXITCODE -ne 0) { Stop-Build "ISCC ist mit Code $LASTEXITCODE fehlgeschlagen." }

$setup = Join-Path $OutDir $setupName
if (-not (Test-Path -LiteralPath $setup)) { Stop-Build "Erwartete Datei fehlt: $setup" }

# ---- Pruefsumme ----
$hash = (Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash.ToLowerInvariant()
$shaFile = Join-Path $OutDir $shaName
[System.IO.File]::WriteAllText($shaFile, "$hash  $setupName`n", $utf8)

foreach ($f in @($setup, $shaFile)) {
    $item = Get-Item -LiteralPath $f
    Write-Host ('{0}  {1:N0} Bytes' -f $item.Name, $item.Length)
}
Write-Host "Version $version, SHA-256 $hash"
