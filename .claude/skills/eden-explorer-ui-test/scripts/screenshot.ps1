param(
    [string]$OutPath = "$env:TEMP\eden_screenshot.png"
)

# Captures the EdenExplorer window to $OutPath via CopyFromScreen - which
# grabs whatever screen pixels are in that rect, NOT necessarily
# EdenExplorer's own content. If another window (e.g. the agent's own
# terminal/app) is occluding EdenExplorer at those same screen coordinates,
# this captures THAT window instead, with no error - it has happened.
# This script therefore verifies EdenExplorer is actually the foreground
# window before trusting the capture; run focus.ps1 first so that's true.
# Always re-read the saved screenshot with the Read tool afterward too -
# never assume what it shows just because this script exited cleanly.
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class EdenShot {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@

$proc = Get-Process EdenExplorer -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle -eq "EdenExplorer" } | Select-Object -First 1
if (-not $proc) { Write-Output "NOT FOUND: EdenExplorer is not running"; exit 1 }

$foreground = [EdenShot]::GetForegroundWindow()
if ($foreground -ne $proc.MainWindowHandle) {
    Write-Output "WARNING: EdenExplorer is not the foreground window right now (something else is on top at its screen position) - run focus.ps1 first, or this capture will show the WRONG window. Capturing anyway so you can see what's there."
}

$rect = New-Object EdenShot+RECT
[EdenShot]::GetWindowRect($proc.MainWindowHandle, [ref]$rect) | Out-Null
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
if ($w -le 0 -or $h -le 0) { Write-Output "BAD RECT: window may be minimized - run focus.ps1 first"; exit 1 }

$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size $w, $h))
$bmp.Save($OutPath)
Write-Output "saved $w x $h to $OutPath"
