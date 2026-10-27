<#
.SYNOPSIS
  Loest die Adressen eines Absturzberichts (absturz-*.txt) zu Funktion, Datei und Zeile auf.

.DESCRIPTION
  Die Windows-Version wird ohne Zeilenangaben ausgeliefert; die stehen in der Symboldatei
  (.pdb), die nur am Release haengt. Der Bericht eines Testers enthaelt am Ende den Abschnitt
  "Adressen (fuer Dennis)": die Ruecksprungadressen als <exe>+0x<Versatz> und die Bau-Kennung
  (PDB-GUID und Age). Dieses Skript

    1. liest diesen Abschnitt,
    2. nimmt die .pdb aus -Pdb oder laedt sie mit -Version vom Release
       (gh release download v<Version> -p planoracad-windows-x64.pdb, in einen eigenen
       Temp-Ordner, nur lesen),
    3. prueft, dass PDB-GUID und Age der .pdb zum Bericht passen (sonst Abbruch, mit -Force
       trotzdem weiter),
    4. loest jeden Rahmen der eigenen exe mit DbgHelp (C:\Windows\System32\dbghelp.dll, Teil
       von Windows) auf, auch eingebettete (inline) Funktionen.

  Ausgabe je Rahmen: Nummer, Adresse, Funktion und Datei:Zeile. Rahmen anderer Module (etwa
  ntdll.dll) stehen nur mit ihrer Adresse da. Laeuft unter Windows PowerShell 5.1 und
  PowerShell 7 (nur Windows).

.EXAMPLE
  powershell -File tools\symbolize.ps1 -Report absturz-2026-10-26_14-03-22.txt -Pdb planoracad-windows-x64.pdb
  powershell -File tools\symbolize.ps1 -Report absturz-2026-10-26_14-03-22.txt -Version 0.2.0-dev.2
#>
param(
    [Parameter(Mandatory = $true)][string]$Report,
    [string]$Pdb = '',
    [string]$Version = '',
    [string]$Repo = 'KaelanTesseract/PlanoraCAD',
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

function Stop-Symbolize([string]$message) {
    [Console]::Error.WriteLine('ABBRUCH: ' + $message)
    exit 1
}

if (-not (Test-Path -LiteralPath $Report -PathType Leaf)) { Stop-Symbolize "Bericht '$Report' nicht gefunden." }
if (($Pdb -eq '') -eq ($Version -eq '')) { Stop-Symbolize 'Genau eins angeben: -Pdb <Pfad> oder -Version <Version>.' }

# --- Bericht lesen ---------------------------------------------------------------------------
$lines = [System.IO.File]::ReadAllLines((Resolve-Path -LiteralPath $Report).Path, [System.Text.Encoding]::UTF8)
$start = -1
for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i].StartsWith('Adressen (f')) { $start = $i; break }
}
if ($start -lt 0) { Stop-Symbolize 'Der Bericht hat keinen Abschnitt "Adressen". Er stammt von einer aelteren Version oder nicht von Windows.' }

$module = ''; $guid = ''; $age = ''; $peTime = ''
$frames = New-Object System.Collections.Generic.List[object]
$reportVersion = ''
foreach ($l in $lines) { if ($l -match '^Version: (.+)$') { $reportVersion = $Matches[1].Trim(); break } }
for ($i = $start + 1; $i -lt $lines.Count; $i++) {
    $l = $lines[$i]
    if ($l -match '^Modul: (.+)$') { $module = $Matches[1].Trim() }
    elseif ($l -match '^PDB-GUID: ([0-9A-Fa-f-]{36})$') { $guid = $Matches[1].ToUpperInvariant() }
    elseif ($l -match '^PDB-Age: (\d+)$') { $age = $Matches[1] }
    elseif ($l -match '^PE-Zeit: (.+)$') { $peTime = $Matches[1].Trim() }
    elseif ($l -match '^\s*(\d+): (.+)\+0x([0-9a-fA-F]+)\s*$') {
        $frames.Add([pscustomobject]@{ N = [int]$Matches[1]; Module = $Matches[2]; Offset = [Convert]::ToUInt64($Matches[3], 16); Text = $l.Trim() })
    }
    elseif ($l -match '^\s*(\d+): (.+)$') {
        $frames.Add([pscustomobject]@{ N = [int]$Matches[1]; Module = ''; Offset = [uint64]0; Text = $l.Trim() })
    }
}
if ($module -eq '') { Stop-Symbolize 'Im Abschnitt "Adressen" fehlt die Zeile "Modul:" (Adressen nicht verfuegbar).' }

# --- .pdb beschaffen ------------------------------------------------------------------------
if ($Version -ne '') {
    if ($Version -notmatch '^\d+\.\d+\.\d+(-(dev|beta)\.\d+)?$') { Stop-Symbolize "Version '$Version' ist ungueltig." }
    if ($reportVersion -ne '' -and $reportVersion -ne $Version) {
        Write-Warning "Der Bericht nennt Version $reportVersion, angegeben ist $Version."
    }
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { Stop-Symbolize 'Die GitHub-Kommandozeile "gh" fehlt. Alternativ die .pdb von Hand laden und -Pdb angeben.' }
    $dir = Join-Path ([System.IO.Path]::GetTempPath()) ('planora-pdb-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $dir | Out-Null
    & gh release download "v$Version" --repo $Repo -p 'planoracad-windows-x64.pdb' -D $dir
    if ($LASTEXITCODE -ne 0) { Stop-Symbolize "Die .pdb zu v$Version liess sich nicht laden (gibt es das Release mit der Datei?)." }
    $Pdb = Join-Path $dir 'planoracad-windows-x64.pdb'
}
if (-not (Test-Path -LiteralPath $Pdb -PathType Leaf)) { Stop-Symbolize "Symboldatei '$Pdb' nicht gefunden." }
$Pdb = (Resolve-Path -LiteralPath $Pdb).Path

# --- DbgHelp --------------------------------------------------------------------------------
if (-not ('PlanoraSym' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class PlanoraSym {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct IMAGEHLP_MODULEW64 {
        public uint SizeOfStruct;
        public ulong BaseOfImage;
        public uint ImageSize;
        public uint TimeDateStamp;
        public uint CheckSum;
        public uint NumSyms;
        public int SymType;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string ModuleName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string ImageName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string LoadedImageName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string LoadedPdbName;
        public uint CVSig;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 780)] public string CVData;
        public uint PdbSig;
        public Guid PdbSig70;
        public uint PdbAge;
        public int PdbUnmatched;
        public int DbgUnmatched;
        public int LineNumbers;
        public int GlobalSymbols;
        public int TypeInfo;
        public int SourceIndexed;
        public int Publics;
        public uint MachineType;
        public uint Reserved;
    }

    [DllImport("dbghelp.dll", SetLastError = true)] static extern uint SymSetOptions(uint o);
    [DllImport("dbghelp.dll", SetLastError = true, CharSet = CharSet.Unicode)] static extern bool SymInitializeW(IntPtr h, string path, bool invade);
    [DllImport("dbghelp.dll", SetLastError = true)] static extern bool SymCleanup(IntPtr h);
    [DllImport("dbghelp.dll", SetLastError = true, CharSet = CharSet.Unicode)] static extern ulong SymLoadModuleExW(IntPtr h, IntPtr file, string image, string module, ulong baseOfDll, uint size, IntPtr data, uint flags);
    [DllImport("dbghelp.dll", SetLastError = true)] static extern bool SymGetModuleInfoW64(IntPtr h, ulong addr, ref IMAGEHLP_MODULEW64 info);
    [DllImport("dbghelp.dll", SetLastError = true)] static extern bool SymFromAddrW(IntPtr h, ulong addr, out ulong disp, IntPtr sym);
    [DllImport("dbghelp.dll", SetLastError = true)] static extern bool SymGetLineFromAddrW64(IntPtr h, ulong addr, out uint disp, IntPtr line);
    [DllImport("dbghelp.dll", SetLastError = true)] static extern uint SymAddrIncludeInlineTrace(IntPtr h, ulong addr);
    [DllImport("dbghelp.dll", SetLastError = true)] static extern bool SymQueryInlineTrace(IntPtr h, ulong start, uint startCtx, ulong startRet, ulong cur, out uint ctx, out uint frameIndex);
    [DllImport("dbghelp.dll", SetLastError = true)] static extern bool SymFromInlineContextW(IntPtr h, ulong addr, uint ctx, out ulong disp, IntPtr sym);
    [DllImport("dbghelp.dll", SetLastError = true)] static extern bool SymGetLineFromInlineContextW(IntPtr h, ulong addr, uint ctx, ulong modBase, out uint disp, IntPtr line);

    const uint SYMOPT_UNDNAME = 0x2, SYMOPT_LOAD_LINES = 0x10, SYMOPT_FAIL_CRITICAL_ERRORS = 0x200, SYMOPT_NO_PROMPTS = 0x80000;
    const int SYM_SIZE = 88, NAME_MAX = 2000, LINE_SIZE = 40;
    public const ulong Base = 0x140000000UL;
    static readonly IntPtr Proc = new IntPtr(0x5150);

    public static IMAGEHLP_MODULEW64 Open(string pdb) {
        SymSetOptions(SYMOPT_UNDNAME | SYMOPT_LOAD_LINES | SYMOPT_FAIL_CRITICAL_ERRORS | SYMOPT_NO_PROMPTS);
        if (!SymInitializeW(Proc, null, false)) throw new Exception("SymInitialize: " + Marshal.GetLastWin32Error());
        ulong b = SymLoadModuleExW(Proc, IntPtr.Zero, pdb, null, Base, 0x40000000, IntPtr.Zero, 0);
        if (b == 0) throw new Exception("SymLoadModuleEx: " + Marshal.GetLastWin32Error());
        IMAGEHLP_MODULEW64 info = new IMAGEHLP_MODULEW64();
        info.SizeOfStruct = (uint)Marshal.SizeOf(typeof(IMAGEHLP_MODULEW64));
        if (!SymGetModuleInfoW64(Proc, Base, ref info)) throw new Exception("SymGetModuleInfo: " + Marshal.GetLastWin32Error());
        return info;
    }

    public static void Close() { SymCleanup(Proc); }

    static string Name(IntPtr sym) {
        int len = Marshal.ReadInt32(sym, 76);
        if (len <= 0) return "?";
        return Marshal.PtrToStringUni(new IntPtr(sym.ToInt64() + 84), Math.Min(len, NAME_MAX - 1));
    }

    static string Line(IntPtr line) {
        IntPtr file = Marshal.ReadIntPtr(line, 24);
        uint no = (uint)Marshal.ReadInt32(line, 16);
        return (file == IntPtr.Zero ? "?" : Marshal.PtrToStringUni(file)) + ":" + no;
    }

    static IntPtr NewSym() {
        IntPtr p = Marshal.AllocHGlobal(SYM_SIZE + NAME_MAX * 2);
        for (int i = 0; i < SYM_SIZE; i += 4) Marshal.WriteInt32(p, i, 0);
        Marshal.WriteInt32(p, 0, SYM_SIZE);
        Marshal.WriteInt32(p, 80, NAME_MAX);
        return p;
    }

    static IntPtr NewLine() {
        IntPtr p = Marshal.AllocHGlobal(LINE_SIZE);
        for (int i = 0; i < LINE_SIZE; i += 4) Marshal.WriteInt32(p, i, 0);
        Marshal.WriteInt32(p, 0, LINE_SIZE);
        return p;
    }

    /// Funktion und Datei:Zeile zur Ruecksprungadresse (Versatz zur Modulbasis), innerste
    /// eingebettete Funktion zuerst.
    public static List<string[]> Resolve(ulong offset) {
        List<string[]> result = new List<string[]>();
        // Eins zurueck: die Ruecksprungadresse zeigt hinter den Aufruf.
        ulong addr = Base + (offset > 0 ? offset - 1 : 0);
        IntPtr sym = NewSym(), line = NewLine();
        try {
            ulong d; uint d32;
            uint inlines = SymAddrIncludeInlineTrace(Proc, addr);
            uint ctx, idx;
            if (inlines > 0 && SymQueryInlineTrace(Proc, addr, 0, addr, addr, out ctx, out idx)) {
                for (uint i = 0; i < inlines; i++) {
                    string fn = SymFromInlineContextW(Proc, addr, ctx + i, out d, sym) ? Name(sym) : "?";
                    string ln = SymGetLineFromInlineContextW(Proc, addr, ctx + i, 0, out d32, line) ? Line(line) : "?";
                    result.Add(new string[] { fn, ln, "inline" });
                }
            }
            string f = SymFromAddrW(Proc, addr, out d, sym) ? Name(sym) : "?";
            string l = SymGetLineFromAddrW64(Proc, addr, out d32, line) ? Line(line) : "?";
            result.Add(new string[] { f, l, "" });
        } finally {
            Marshal.FreeHGlobal(sym);
            Marshal.FreeHGlobal(line);
        }
        return result;
    }
}
'@
}

function Get-ShortPath([string]$path) {
    # Bauordner (lokal oder auf GitHub) weglassen: ab "crates\" bzw. "library\" (Rust selbst).
    $p = $path -replace '/', '\'
    # Fremde Pakete: ab dem Paketordner (z. B. egui-0.36.2\src\context.rs).
    $k = $p.IndexOf('\registry\src\')
    if ($k -ge 0) {
        $rest = $p.Substring($k + 14)
        $j = $rest.IndexOf('\')
        if ($j -ge 0) { return $rest.Substring($j + 1) }
    }
    foreach ($mark in @('\crates\', '\library\')) {
        $k = $p.IndexOf($mark)
        if ($k -ge 0) { return $p.Substring($k + 1) }
    }
    return $p
}

try {
    $info = [PlanoraSym]::Open($Pdb)
    $pdbGuid = $info.PdbSig70.ToString().ToUpperInvariant()
    Write-Output "Bericht:  $Report"
    Write-Output "Symbole:  $Pdb"
    Write-Output "Modul:    $module   Version: $reportVersion   PE-Zeit: $peTime"
    Write-Output "PDB-GUID: Bericht $guid (Age $age), Symboldatei $pdbGuid (Age $($info.PdbAge))"
    if ($guid -eq '') {
        Write-Warning 'Der Bericht nennt keine PDB-GUID; ob die Symboldatei passt, ist nicht geprueft.'
    } elseif ($guid -ne $pdbGuid -or ($age -ne '' -and [uint32]$age -ne $info.PdbAge)) {
        if (-not $Force) { Stop-Symbolize 'Die Symboldatei passt nicht zum Bericht (andere PDB-GUID oder Age). Mit -Force trotzdem aufloesen (Ergebnis dann unzuverlaessig).' }
        Write-Warning 'Die Symboldatei passt nicht zum Bericht; das Ergebnis ist unzuverlaessig.'
    } else {
        Write-Output 'Die Symboldatei passt zum Bericht.'
    }
    if ($info.LineNumbers -eq 0) { Write-Warning 'Die Symboldatei enthaelt keine Zeilenangaben.' }
    Write-Output ''
    Write-Output 'Ablauf (innerste Stelle zuerst; "aus" = eingebettet in die Funktion darunter):'
    foreach ($f in $frames) {
        if ($f.Module -ne $module) {
            Write-Output ('{0,3}: {1}' -f $f.N, ($f.Text -replace '^\d+:\s*', ''))
            continue
        }
        $first = $true
        foreach ($r in [PlanoraSym]::Resolve($f.Offset)) {
            $head = if ($first) { '{0,3}: +0x{1:x}' -f $f.N, $f.Offset } else { '      aus' }
            $first = $false
            Write-Output ('{0}  {1}' -f $head, $r[0])
            Write-Output ('         bei {0}' -f (Get-ShortPath $r[1]))
        }
    }
} finally {
    if ('PlanoraSym' -as [type]) { [PlanoraSym]::Close() }
}
