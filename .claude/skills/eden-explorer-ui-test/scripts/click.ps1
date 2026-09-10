param(
    [Parameter(Mandatory=$true)][int]$X,
    [Parameter(Mandatory=$true)][int]$Y,
    [ValidateSet("left", "right")][string]$Button = "left"
)

# Posts a single click at client coordinates (X, Y) - the origin is the
# window's own top-left, same frame as a screenshot.ps1 image. Reliable for
# plain single left/right clicks WITHOUT modifier keys.
#
# NOT reliable for: double-clicks (two calls to this script in a row do not
# reliably register as a double-click in egui - use send-keys.ps1 with
# {ENTER} on an already-selected row to "open" it instead), and clicks that
# need Ctrl/Shift/Alt held (PostMessage doesn't update the real OS modifier
# key state table, so egui sees the click but not the modifier - use
# send-keys.ps1's modifier syntax, or drag.ps1, for those).
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class EdenClick {
  [DllImport("user32.dll")] public static extern IntPtr PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
}
"@

$proc = Get-Process EdenExplorer -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle -eq "EdenExplorer" } | Select-Object -First 1
if (-not $proc) { Write-Output "NOT FOUND: EdenExplorer is not running"; exit 1 }
$hwnd = $proc.MainWindowHandle
$lparam = [IntPtr](($Y -shl 16) -bor ($X -band 0xFFFF))

[EdenClick]::PostMessage($hwnd, 0x0200, [IntPtr]::Zero, $lparam) | Out-Null  # WM_MOUSEMOVE

if ($Button -eq "right") {
    Start-Sleep -Milliseconds 80
    [EdenClick]::PostMessage($hwnd, 0x0204, [IntPtr]0x0002, $lparam) | Out-Null  # WM_RBUTTONDOWN
    Start-Sleep -Milliseconds 60
    [EdenClick]::PostMessage($hwnd, 0x0205, [IntPtr]::Zero, $lparam) | Out-Null  # WM_RBUTTONUP
} else {
    Start-Sleep -Milliseconds 80
    [EdenClick]::PostMessage($hwnd, 0x0201, [IntPtr]0x0001, $lparam) | Out-Null  # WM_LBUTTONDOWN
    Start-Sleep -Milliseconds 60
    [EdenClick]::PostMessage($hwnd, 0x0202, [IntPtr]::Zero, $lparam) | Out-Null  # WM_LBUTTONUP
}

Write-Output "posted $Button click at client $X,$Y"
