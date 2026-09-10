param(
    [Parameter(Mandatory=$true)][int]$X1,
    [Parameter(Mandatory=$true)][int]$Y1,
    [Parameter(Mandatory=$true)][int]$X2,
    [Parameter(Mandatory=$true)][int]$Y2
)

# Left-button drag from (X1,Y1) to (X2,Y2) in client coordinates, moving in a
# few intermediate steps so egui's drag-vs-click threshold registers it as a
# real drag. Primarily useful for text-selection drags (e.g. in the preview
# pane) and range/box selection.
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class EdenDrag {
  [DllImport("user32.dll")] public static extern IntPtr PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
}
"@

$proc = Get-Process EdenExplorer -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle -eq "EdenExplorer" } | Select-Object -First 1
if (-not $proc) { Write-Output "NOT FOUND: EdenExplorer is not running"; exit 1 }
$hwnd = $proc.MainWindowHandle

function EdenLParam($x, $y) { [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF)) }

[EdenDrag]::PostMessage($hwnd, 0x0200, [IntPtr]::Zero, (EdenLParam $X1 $Y1)) | Out-Null
Start-Sleep -Milliseconds 60
[EdenDrag]::PostMessage($hwnd, 0x0201, [IntPtr]0x0001, (EdenLParam $X1 $Y1)) | Out-Null  # WM_LBUTTONDOWN
Start-Sleep -Milliseconds 60

for ($i = 1; $i -le 5; $i++) {
    $xi = $X1 + (($X2 - $X1) * $i / 5)
    $yi = $Y1 + (($Y2 - $Y1) * $i / 5)
    [EdenDrag]::PostMessage($hwnd, 0x0200, [IntPtr]0x0001, (EdenLParam $xi $yi)) | Out-Null  # WM_MOUSEMOVE (button held)
    Start-Sleep -Milliseconds 40
}

[EdenDrag]::PostMessage($hwnd, 0x0202, [IntPtr]::Zero, (EdenLParam $X2 $Y2)) | Out-Null  # WM_LBUTTONUP
Write-Output "dragged from $X1,$Y1 to $X2,$Y2"
