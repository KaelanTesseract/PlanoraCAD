<#
.SYNOPSIS
  Baut den Installer planoracad-setup-x64.exe (Graphit-Huelle mit eingebettetem Inno-Motor) und schreibt die Pruefsumme dazu.

.DESCRIPTION
  Vorgaben: Programmdatei target\release\planora-app.exe, Lizenzdatei dist\THIRD-PARTY-LICENSES.txt
  (fehlt sie, wird sie mit tools\third-party-licenses.ps1 erzeugt), Ausgabeordner dist.
  Die Version kommt aus [workspace.package] in Cargo.toml.
  Ablauf: Inno Setup 6.7 oder neuer baut den Motor target\installer-engine\planoracad-engine-x64.exe
  (nur dort, nie in <OutDir>); danach baut cargo die Huelle planora-setup, die den Motor einbettet
  (PLANORA_SETUP_ENGINE); die Huelle wird als <OutDir>\planoracad-setup-x64.exe abgelegt.
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
    Stop-Build 'ISCC.exe (Inno Setup 6) nicht gefunden. Inno Setup 6.7 oder neuer installieren (https://jrsoftware.org/isdl.php) oder -Iscc <Pfad> angeben.'
}

# Die Huelle braucht die Funktionen von Inno Setup 6.7. Die Version steht meist in ProductVersion
# von ISCC.exe; fehlt sie dort (0.0.0.0), liefert ein Probelauf "Compiler engine version: ...".
function Get-InnoVersion([string]$iscc) {
    $raw = [string](Get-Item -LiteralPath $iscc).VersionInfo.ProductVersion
    $mm = [regex]::Match($raw, '^\s*(\d+)\.(\d+)')
    if ($mm.Success -and ([version]('{0}.{1}' -f $mm.Groups[1].Value, $mm.Groups[2].Value)) -gt [version]'0.0') {
        return $raw.Trim()
    }
    $probe = Join-Path ([System.IO.Path]::GetTempPath()) ('planora-iscc-probe-' + [guid]::NewGuid().ToString('N') + '.iss')
    try {
        [System.IO.File]::WriteAllText($probe, "[Setup]`nAppName=Probe`nAppVersion=1`nDefaultDirName=Probe`n", $utf8)
        $out = & $iscc /O- $probe 2>&1 | Out-String
    }
    finally {
        Remove-Item -LiteralPath $probe -Force -ErrorAction SilentlyContinue
        $global:LASTEXITCODE = 0
    }
    $vm = [regex]::Match($out, 'Compiler engine version:\s*Inno Setup\s+(\d+\.\d+(\.\d+)?)')
    if ($vm.Success) { return $vm.Groups[1].Value }
    return $raw.Trim()
}
$innoRaw = Get-InnoVersion $isccPath
$innoMatch = [regex]::Match($innoRaw, '^\s*(\d+)\.(\d+)')
if (-not $innoMatch.Success) {
    Stop-Build "Inno-Setup-Version nicht lesbar ('$innoRaw'). Inno Setup 6.7 oder neuer noetig."
}
$innoVersion = [version]('{0}.{1}' -f $innoMatch.Groups[1].Value, $innoMatch.Groups[2].Value)
if ($innoVersion -lt [version]'6.7') {
    Stop-Build "Gefunden: Inno Setup $innoRaw. Inno Setup 6.7 oder neuer noetig."
}

if (-not (Test-Path -LiteralPath $Licenses)) {
    $dir = Split-Path -Parent $Licenses
    if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
    & (Join-Path $PSScriptRoot 'third-party-licenses.ps1') -Output $Licenses
    if (-not (Test-Path -LiteralPath $Licenses)) { Stop-Build "Lizenzdatei konnte nicht erzeugt werden: $Licenses" }
}

if (-not (Test-Path -LiteralPath $OutDir)) { New-Item -ItemType Directory -Path $OutDir | Out-Null }

# ---- Motor bauen (Inno Setup, nur unter target\installer-engine, nie in OutDir) ----
$engineDir = Join-Path $root 'target\installer-engine'
$engine = Join-Path $engineDir 'planoracad-engine-x64.exe'
if (-not (Test-Path -LiteralPath $engineDir)) { New-Item -ItemType Directory -Path $engineDir | Out-Null }
if (Test-Path -LiteralPath $engine) { Remove-Item -LiteralPath $engine -Force }
$script = Join-Path $root 'installer\planoracad.iss'
# Keine Anfuehrungszeichen innerhalb der Argumente: PowerShell ab 7.3 ($PSNativeCommandArgumentPassing)
# maskiert sie mit \", und ISCC lehnt den Namen dann ab. Jedes Argument ist ein eigener String; hat
# es Leerzeichen (Pfade), setzen PowerShell 5.1 und 7 es als Ganzes in Anfuehrungszeichen.
& $isccPath /Qp "/DAppVersion=$version" "/DAppVersionNumeric=$versionNumeric" "/DSourceExe=$Exe" "/DLicensesFile=$Licenses" /Fplanoracad-engine-x64 "/O$engineDir" $script
if ($LASTEXITCODE -ne 0) { Stop-Build "ISCC ist mit Code $LASTEXITCODE fehlgeschlagen." }
if (-not (Test-Path -LiteralPath $engine)) { Stop-Build "Motor fehlt: $engine" }

# ---- Huelle bauen (bettet den Motor ein) ----
# cargo legt die Huelle unter CARGO_TARGET_DIR ab, wenn gesetzt (sonst unter target). Eine alte
# Huelle wird vorher geloescht, damit nie eine veraltete Datei nach <OutDir> gelangt.
if ($env:CARGO_TARGET_DIR) { $targetDir = Resolve-Full $env:CARGO_TARGET_DIR }
else { $targetDir = Join-Path $root 'target' }
$shell = Join-Path $targetDir 'release\planoracad-setup.exe'
if (Test-Path -LiteralPath $shell) { Remove-Item -LiteralPath $shell -Force }
$oldEngine = $env:PLANORA_SETUP_ENGINE
$env:PLANORA_SETUP_ENGINE = $engine
try {
    & cargo build --release --locked -p planora-setup
    $cargoCode = $LASTEXITCODE
}
finally {
    if ($null -eq $oldEngine) { Remove-Item Env:\PLANORA_SETUP_ENGINE -ErrorAction SilentlyContinue }
    else { $env:PLANORA_SETUP_ENGINE = $oldEngine }
}
if ($cargoCode -ne 0) { Stop-Build "cargo build fuer planora-setup ist mit Code $cargoCode fehlgeschlagen." }
if (-not (Test-Path -LiteralPath $shell)) { Stop-Build "Huelle fehlt: $shell" }

$setup = Join-Path $OutDir $setupName
Copy-Item -LiteralPath $shell -Destination $setup -Force
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
