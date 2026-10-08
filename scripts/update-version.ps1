<#
.SYNOPSIS
  Sets the package version in Cargo.toml and Cargo.lock.

.DESCRIPTION
  Both files have to move together: the release build runs with --locked, which
  fails if Cargo.lock still records the old version.

  This only edits files - committing and tagging is the workflow's job, so the
  script stays runnable locally without touching git.

  The version matters beyond packaging: the app compares a release's tag against
  its own CARGO_PKG_VERSION, so the binary has to be built after this runs or it
  would offer the same update for ever.
#>
param(
  [Parameter(Mandatory = $true)][string]$Version
)

$ErrorActionPreference = "Stop"

if ($Version -notmatch '^\d+\.\d+\.\d+$') {
  throw "Version '$Version' is not MAJOR.MINOR.PATCH. The updater only understands three plain numbers."
}

# Cargo.toml: the one line that starts with `version =` is [package].version -
# dependency versions are all inline (`foo = { version = "..." }`), so they are
# never at the start of a line.
$toml = Get-Content Cargo.toml
$hits = ($toml | Select-String -Pattern '^version\s*=' ).Count
if ($hits -ne 1) { throw "Expected exactly one top-level 'version =' line in Cargo.toml, found $hits." }
$toml -replace '^version\s*=\s*".*"', "version = `"$Version`"" | Set-Content Cargo.toml -Encoding utf8

# Cargo.lock: only the EdenExplorer package's own entry, which is the `version`
# line immediately after its `name = "EdenExplorer"` line.
$lock = Get-Content Cargo.lock
$index = -1
for ($i = 0; $i -lt $lock.Count; $i++) {
  if ($lock[$i] -eq 'name = "EdenExplorer"') { $index = $i; break }
}
if ($index -lt 0) { throw 'Could not find the EdenExplorer package entry in Cargo.lock.' }
if ($lock[$index + 1] -notmatch '^version\s*=') {
  throw "Cargo.lock line $($index + 2) is not the version line for EdenExplorer."
}
$lock[$index + 1] = "version = `"$Version`""
$lock | Set-Content Cargo.lock -Encoding utf8

Write-Host "Cargo.toml : $((Select-String -Path Cargo.toml -Pattern '^version\s*=').Line)"
Write-Host "Cargo.lock : $($lock[$index + 1])"
exit 0
