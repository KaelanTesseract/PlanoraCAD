param([string]$Cmd = "shot", [int]$X = 0, [int]$Y = 0, [string]$Text = "")
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
if (-not ("W" -as [type])) {
Add-Type @"
using System;using System.Runtime.InteropServices;
public class W{[DllImport("user32.dll")]public static extern bool GetWindowRect(IntPtr h,out RECT r);[DllImport("user32.dll")]public static extern bool SetForegroundWindow(IntPtr h);
[DllImport("user32.dll")]public static extern bool SetCursorPos(int x,int y);[DllImport("user32.dll")]public static extern void mouse_event(int f,int x,int y,int d,int e);
public struct RECT{public int L,T,R,B;}}
"@
}
$p = Get-Process planora-app -ErrorAction SilentlyContinue
if (-not $p) { "APP NOT RUNNING (crashed?)"; exit 2 }
[W]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
$r = New-Object W+RECT; [W]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
function Click($x, $y, $n = 1) {
  [W]::SetCursorPos($r.L + $x, $r.T + $y) | Out-Null; Start-Sleep -Milliseconds 150
  for ($i = 0; $i -lt $n; $i++) { [W]::mouse_event(2,0,0,0,0); Start-Sleep -Milliseconds 60; [W]::mouse_event(4,0,0,0,0); Start-Sleep -Milliseconds 80 }
}
switch ($Cmd) {
  "click" { Click $X $Y }
  "dclick" { Click $X $Y 2 }
  "rclick" { [W]::SetCursorPos($r.L + $X, $r.T + $Y) | Out-Null; Start-Sleep -Milliseconds 150; [W]::mouse_event(8,0,0,0,0); Start-Sleep -Milliseconds 60; [W]::mouse_event(16,0,0,0,0) }
  "drag" { # X Y = start, Text = "x2,y2,button(l|r|m)"
    $a = $Text.Split(","); $x2 = [int]$a[0]; $y2 = [int]$a[1]
    $dn = 2; $up = 4; if ($a[2] -eq "r") { $dn = 8; $up = 16 } elseif ($a[2] -eq "m") { $dn = 32; $up = 64 }
    [W]::SetCursorPos($r.L + $X, $r.T + $Y) | Out-Null; Start-Sleep -Milliseconds 150
    [W]::mouse_event($dn,0,0,0,0)
    for ($i = 1; $i -le 20; $i++) { [W]::SetCursorPos($r.L + $X + [int](($x2 - $X) * $i / 20), $r.T + $Y + [int](($y2 - $Y) * $i / 20)) | Out-Null; Start-Sleep -Milliseconds 25 }
    [W]::mouse_event($up,0,0,0,0) }
  "wheel" { [W]::SetCursorPos($r.L + $X, $r.T + $Y) | Out-Null; Start-Sleep -Milliseconds 100; [W]::mouse_event(0x800,0,0,[int]$Text,0) }
  "move" { for ($i = 0; $i -lt 30; $i++) { [W]::SetCursorPos($r.L + $X + $i * 20, $r.T + $Y + ($i % 5) * 40) | Out-Null; Start-Sleep -Milliseconds 30 } }
  "type" { [System.Windows.Forms.SendKeys]::SendWait($Text) }
}
Start-Sleep -Milliseconds 700
$p = Get-Process planora-app -ErrorAction SilentlyContinue
if (-not $p) { "APP DIED after $Cmd $X $Y $Text"; exit 3 }
$b = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
$g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen($r.L, $r.T, 0, 0, $b.Size)
$b.Save("$env:TEMP\planora_shot.png"); "ok"
