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
#
# The window is found by walking the top-level windows rather than via
# Process.MainWindowHandle. While the app is MINIMIZED, MainWindowHandle
# resolves to a small untitled helper window this process also owns, not the
# real one - and MainWindowTitle then reads empty, so the old
# `Where-Object { $_.MainWindowTitle -eq "EdenExplorer" }` filter matched
# nothing and this script reported "EdenExplorer is not running" for an app
# that was running perfectly well (the false negative recorded in CLAUDE.md).
# Focusing that helper also did nothing visible, which looks identical to
# "focus silently failed".
#
# Matching on the title alone is not enough either: a Windows Explorer window
# showing a folder *named* EdenExplorer has exactly that title, so the owning
# process is checked as well.
Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public class EdenFocus {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr hWnd, StringBuilder text, int count);

  delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr lParam);

  static List<uint> wantedPids;
  static IntPtr found;

  static bool Visit(IntPtr hWnd, IntPtr lParam) {
    if (!IsWindowVisible(hWnd)) return true;
    uint pid; GetWindowThreadProcessId(hWnd, out pid);
    if (!wantedPids.Contains(pid)) return true;
    var title = new StringBuilder(256);
    GetWindowTextW(hWnd, title, title.Capacity);
    if (title.ToString() != "EdenExplorer") return true;
    found = hWnd;
    return false;
  }

  // The real window: owned by one of EdenExplorer.exe's processes AND titled
  // "EdenExplorer". Works whether the window is minimized or not.
  public static IntPtr FindWindow(uint[] pids) {
    wantedPids = new List<uint>(pids);
    found = IntPtr.Zero;
    EnumWindows(Visit, IntPtr.Zero);
    return found;
  }
}
"@

$procs = @(Get-Process EdenExplorer -ErrorAction SilentlyContinue)
if ($procs.Count -eq 0) {
    Write-Output "NOT FOUND: EdenExplorer is not running"
    exit 1
}

$pids = [uint32[]]($procs | ForEach-Object { [uint32]$_.Id })
$target = [EdenFocus]::FindWindow($pids)
if ($target -eq [IntPtr]::Zero) {
    # The process is alive but has no window titled EdenExplorer yet - it is
    # still starting up, or the title has not been set (see
    # `windowsoverrides::apply_window_override`).
    Write-Output "NOT FOUND: EdenExplorer is running (pid $($procs[0].Id)) but has no window titled 'EdenExplorer' yet"
    exit 1
}

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

Write-Output "focused EdenExplorer (pid $($procs[0].Id), hwnd $target)"
exit 0
