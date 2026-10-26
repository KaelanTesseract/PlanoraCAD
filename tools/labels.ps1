<#
.SYNOPSIS
  Legt die Labels fuer Rueckmeldungen im GitHub-Repository an (bug, idee, frage, Sketch, 3D,
  Export, Tastenkuerzel). Vorhandene Labels werden uebersprungen.

.DESCRIPTION
  Braucht die GitHub-CLI (gh), angemeldet mit Schreibrecht auf das Repository.
  Mit -Probelauf werden nur die Befehle ausgegeben; gh wird nicht aufgerufen, es gibt keinen
  Netzzugriff. Laeuft unter Windows PowerShell 5.1 und PowerShell 7.

.EXAMPLE
  powershell -File tools\labels.ps1 -Probelauf
  powershell -File tools\labels.ps1
#>
param(
    [switch]$Probelauf,
    [string]$Repo = 'KaelanTesseract/PlanoraCAD'
)

$ErrorActionPreference = 'Stop'

# Umlaute als Zeichencode, damit die Datei auch ohne BOM in Windows PowerShell 5.1 stimmt.
$oe = [string][char]0xF6
$kuerzel = 'Tastenk' + [char]0xFC + 'rzel'

# Name, Farbe (ohne #), Beschreibung
$labels = @(
    @('bug', 'd73a4a', 'Etwas funktioniert nicht'),
    @('idee', 'a2eeef', 'Ein Wunsch oder eine Idee'),
    @('frage', 'd876e3', 'Eine Frage'),
    @('Sketch', '0e8a16', 'Betrifft die Skizze'),
    @('3D', '1d76db', ('Betrifft die 3D-Ansicht und die K' + $oe + 'rper')),
    @('Export', 'fbca04', 'Betrifft Export und Import'),
    @($kuerzel, '5319e7', ('Betrifft die ' + $kuerzel))
)

$vorhanden = @()
if (-not $Probelauf) {
    $json = & gh label list --repo $Repo --limit 200 --json name
    if ($LASTEXITCODE -ne 0) { throw 'gh label list ist fehlgeschlagen.' }
    $vorhanden = @(($json | ConvertFrom-Json) | ForEach-Object { $_.name })
}

foreach ($l in $labels) {
    $name = $l[0]; $farbe = $l[1]; $text = $l[2]
    if ($vorhanden -contains $name) {
        Write-Output "Uebersprungen (gibt es schon): $name"
        continue
    }
    if ($Probelauf) {
        Write-Output "gh label create `"$name`" --repo $Repo --color $farbe --description `"$text`""
    } else {
        & gh label create $name --repo $Repo --color $farbe --description $text
        if ($LASTEXITCODE -ne 0) { throw "gh label create $name ist fehlgeschlagen." }
        Write-Output "Angelegt: $name"
    }
}
