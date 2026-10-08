<#
.SYNOPSIS
  Builds the release binary and assembles the files a release publishes.

.DESCRIPTION
  The app's update check reads exactly these names, so they are a contract, not
  a convention:

    EdenExplorer.exe                         the program, downloaded in place
    SHA256SUMS.txt                           checked before anything is replaced
    EdenExplorer-<version>-windows-x64.zip   for people who update by hand

  An in-app updater must refuse to install a release that has no SHA256SUMS.txt,
  so never publish one of these without the other.

  Run scripts/update-version.ps1 first: the binary's own version comes from
  Cargo.toml at build time (src/build.rs feeds CARGO_PKG_VERSION to winres).
#>
param(
  [Parameter(Mandatory = $true)][string]$Version
)

$ErrorActionPreference = "Stop"

cargo build --release --locked
if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE." }

$built = "target/release/EdenExplorer.exe"
if (-not (Test-Path $built)) { throw "No binary at $built after a successful build." }

# A release's assets are everything in this directory, so it must contain only
# the files meant to be downloadable - start it empty.
$dist = "release/dist"
if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory -Path $dist -Force | Out-Null

Copy-Item $built "$dist/EdenExplorer.exe"

# The zip carries the docs; the bare .exe above is what an updater replaces.
$staging = "release/zip-staging"
if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }
New-Item -ItemType Directory -Path $staging -Force | Out-Null
Copy-Item $built "$staging/EdenExplorer.exe"
foreach ($doc in @("README.md", "LICENSE")) {
  if (Test-Path $doc) { Copy-Item $doc $staging }
}
Compress-Archive -Path "$staging/*" -DestinationPath "$dist/EdenExplorer-$Version-windows-x64.zip"
Remove-Item $staging -Recurse -Force

# `sha256sum` layout - two spaces between hash and name - so the file is both
# machine-readable by the app and usable with `sha256sum -c`.
Get-ChildItem $dist -File | ForEach-Object {
  $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
  "$hash  $($_.Name)"
} | Set-Content "$dist/SHA256SUMS.txt" -Encoding ascii

Write-Host "Packaged release $Version"
Get-ChildItem $dist -File | ForEach-Object { Write-Host ("  {0,-46} {1,12:N0} bytes" -f $_.Name, $_.Length) }
Write-Host "--- SHA256SUMS.txt ---"
Get-Content "$dist/SHA256SUMS.txt" | ForEach-Object { Write-Host "  $_" }
exit 0
