param(
    [ValidateSet("release", "debug")][string]$Profile = "release",
    [switch]$SkipBuild
)

# Stops any running EdenExplorer.exe (release build locks its own .exe file,
# so a stale running instance makes `cargo build --release` fail with
# "Access is denied"), builds, then launches the fresh binary.
#
# Use -Profile debug when you need to see eprintln!/println! output (a debug
# build keeps its console attached; the release build sets
# `windows_subsystem = "windows"` under `cfg(not(debug_assertions))`, which
# hides the console). Combine with `Start-Process -RedirectStandardOutput /
# -RedirectStandardError` to capture that output to a file instead of a
# console window - see the skill's SKILL.md for the exact pattern.
$repoRoot = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)))

Stop-Process -Name EdenExplorer -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1

if (-not $SkipBuild) {
    Push-Location $repoRoot
    if ($Profile -eq "release") {
        cargo build --release
    } else {
        cargo build
    }
    $buildExit = $LASTEXITCODE
    Pop-Location
    if ($buildExit -ne 0) { Write-Output "BUILD FAILED (exit $buildExit)"; exit $buildExit }
}

$exePath = Join-Path $repoRoot "target\$Profile\EdenExplorer.exe"
Start-Process -FilePath $exePath
Start-Sleep -Seconds 2

$proc = Get-Process EdenExplorer -ErrorAction SilentlyContinue
if ($proc) {
    Write-Output "launched EdenExplorer (pid $($proc.Id)) from $exePath"
} else {
    Write-Output "FAILED TO LAUNCH: $exePath"
    exit 1
}
