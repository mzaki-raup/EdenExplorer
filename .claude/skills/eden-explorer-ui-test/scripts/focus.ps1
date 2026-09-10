# Forces real OS foreground focus onto the running EdenExplorer.exe window.
#
# Why this exists: plain SetForegroundWindow() is silently ignored by Windows
# ("foreground lock") when called from a process (like a PowerShell child
# process spawned by an agent) that wasn't itself the last-focused app. The
# fix is the classic AttachThreadInput trick: temporarily attach this
# process's input queue to the current foreground thread's input queue, which
# grants permission to steal focus, then detach again immediately.
#
# Call this before send-keys.ps1 (SendKeys always goes to whatever window has
# *real* OS focus, not whatever click.ps1 last posted to) and before
# screenshot.ps1 if you need the window raised above other windows first.
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class EdenFocus {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
}
"@

$proc = Get-Process EdenExplorer -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle -eq "EdenExplorer" } | Select-Object -First 1
if (-not $proc) { Write-Output "NOT FOUND: EdenExplorer is not running (or has no visible window yet)"; exit 1 }

$target = $proc.MainWindowHandle
$fg = [EdenFocus]::GetForegroundWindow()
$fgThread = 0
[EdenFocus]::GetWindowThreadProcessId($fg, [ref]$fgThread) | Out-Null
$curThread = [EdenFocus]::GetCurrentThreadId()

[EdenFocus]::AttachThreadInput($curThread, $fgThread, $true) | Out-Null
[EdenFocus]::ShowWindow($target, 9) | Out-Null   # SW_RESTORE, in case it's minimized
[EdenFocus]::BringWindowToTop($target) | Out-Null
[EdenFocus]::SetForegroundWindow($target) | Out-Null
[EdenFocus]::AttachThreadInput($curThread, $fgThread, $false) | Out-Null

Start-Sleep -Milliseconds 200

# Verify it actually worked - the calls above can all return success/true
# and still not have moved EdenExplorer above whatever else is on screen
# (this has been observed to silently leave another app, e.g. the agent's
# own terminal/app window, occluding EdenExplorer at the same screen
# coordinates - a screenshot taken in that state captures the WRONG window's
# pixels with no error of any kind). Don't trust the calls above; check.
$actual = [EdenFocus]::GetForegroundWindow()
if ($actual -ne $target) {
    Write-Output "WARNING: focus did not stick - foreground window is still $actual, not EdenExplorer ($target). A screenshot right now would capture the wrong window. Try again, or check what's stealing focus."
    exit 1
}

Write-Output "focused EdenExplorer (pid $($proc.Id))"
exit 0
