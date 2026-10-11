<#
.SYNOPSIS
  Works out the next release version from the commits since the last v* tag,
  and writes the release notes for them.

.DESCRIPTION
  This repo does not use conventional commits - subjects read as plain English
  ("Add a context menu order", "Fix the icon cache"). The leading verb is what
  says how big a change it is, so that is what the rules below read. Conventional
  `feat:`/`fix:`/`BREAKING CHANGE:` are understood too, so both styles work.

  A verb that is not listed anywhere falls through to a patch bump on purpose:
  the failure mode is an extra patch release, never the release that silently
  never shipped.

  Prints nothing to stdout except the results, and appends them to
  $env:GITHUB_OUTPUT when running under Actions:
    version   the version to release, or empty when nothing warrants one
    tag       v<version>
    previous  the tag it is measured from, or empty on a first release

  With -Tag, nothing is decided: the version is the tag's own, and the notes
  describe the commits between the previous v* tag and that tag. This is what
  the GitHub workflow runs, which only ever builds a tag the Forgejo workflow
  has already cut - so both forges publish the same version with the same
  notes, from the same script.
#>
param(
  # Used when no v* tag exists yet, instead of bumping Cargo.toml's version.
  [string]$FirstVersion = "1.0.0",
  [string]$NotesFile = "notes.md",
  # An existing v* tag to write the notes for, instead of planning a new one.
  [string]$Tag = ""
)

$ErrorActionPreference = "Stop"

# A new user-facing capability -> minor.
$MinorVerbs = @("Add", "Introduce", "Implement", "Support", "Redesign", "Enable", "feat")
# Nothing a user would notice -> no release of its own.
$NoReleaseVerbs = @("Document", "docs", "chore", "ci", "style", "test", "refactor", "Merge")

function Get-Verb([string]$subject) {
  # The leading word, less any conventional-commit scope/bang: "chore(release):"
  # and "feat!:" both reduce to their type.
  if ($subject -match '^([A-Za-z][\w-]*)') { return $Matches[1] }
  return ""
}

if ($Tag) {
  if ($Tag -notmatch '^v\d+\.\d+\.\d+$') { throw "'$Tag' is not a vMAJOR.MINOR.PATCH tag." }
  if (-not (git tag --list $Tag)) { throw "tag '$Tag' does not exist here - fetch the tags first." }
  $wanted = [version]($Tag -replace '^v', '')
  # The newest tag that is older than this one by version - not "the newest
  # tag", which is this one.
  $previous = (git tag --list "v*" --sort=-v:refname |
    Where-Object { $_ -match '^v\d+\.\d+\.\d+$' -and [version]($_ -replace '^v', '') -lt $wanted } |
    Select-Object -First 1)
  $range = if ($previous) { "$previous..$Tag" } else { $Tag }
} else {
  $previous = (git tag --list "v*" --sort=-v:refname | Select-Object -First 1)
  $range = if ($previous) { "$previous..HEAD" } else { "HEAD" }
}

# -z plus a record separator: subjects can contain anything, including quotes.
$raw = git log --no-merges --format="%H%x1f%s%x1f%b%x1e" $range
$records = ($raw -split "`u{1e}") | Where-Object { $_.Trim() -ne "" }

$bump = 0   # 0 none, 1 patch, 2 minor, 3 major
$features = @(); $fixes = @(); $other = @()

foreach ($record in $records) {
  $parts = $record.TrimStart("`r", "`n") -split "`u{1f}"
  $hash = $parts[0]; $subject = $parts[1]; $body = if ($parts.Count -gt 2) { $parts[2] } else { "" }
  if (-not $subject) { continue }

  $verb = Get-Verb $subject
  $breaking = ($body -match 'BREAKING[ -]CHANGE') -or ($subject -match '^[A-Za-z][\w-]*(\([^)]*\))?!:')
  $line = "- $subject ($($hash.Substring(0,8)))"

  if ($breaking) {
    $bump = [Math]::Max($bump, 3); $features += $line; continue
  }
  if ($NoReleaseVerbs -contains $verb) { continue }
  if ($MinorVerbs -contains $verb) {
    $bump = [Math]::Max($bump, 2); $features += $line; continue
  }
  if ($verb -in @("Fix", "fix", "perf")) {
    $bump = [Math]::Max($bump, 1); $fixes += $line; continue
  }
  $bump = [Math]::Max($bump, 1); $other += $line
}

if ($bump -eq 0 -and $Tag) {
  # The tag exists, so it is being published regardless - say so rather than
  # refuse. The Forgejo workflow never cuts a tag for nothing; this is only
  # reachable for a tag made by hand.
  $other += "- Release $Tag"
  $bump = 1
}

if ($bump -eq 0) {
  Write-Host "No commits since $previous warrant a release."
  if ($env:GITHUB_OUTPUT) {
    "version=" | Out-File $env:GITHUB_OUTPUT -Append -Encoding utf8
    "tag="     | Out-File $env:GITHUB_OUTPUT -Append -Encoding utf8
  }
  exit 0
}

if ($Tag) {
  $version = $Tag -replace '^v', ''
} elseif (-not $previous) {
  $version = $FirstVersion
} else {
  $base = $previous -replace '^v', ''
  $n = $base -split '\.'
  [int]$major = $n[0]; [int]$minor = $n[1]; [int]$patch = $n[2]
  switch ($bump) {
    3 { $major++; $minor = 0; $patch = 0 }
    2 { $minor++; $patch = 0 }
    1 { $patch++ }
  }
  $version = "$major.$minor.$patch"
}

$sections = @()
if ($features.Count) { $sections += "### Features`n`n" + ($features -join "`n") }
if ($fixes.Count)    { $sections += "### Fixes`n`n"    + ($fixes    -join "`n") }
if ($other.Count)    { $sections += "### Other changes`n`n" + ($other -join "`n") }
($sections -join "`n`n") | Set-Content $NotesFile -Encoding utf8

$levels = @{ 1 = "patch"; 2 = "minor"; 3 = "major" }
Write-Host "Previous tag : $(if ($previous) { $previous } else { '(none)' })"
if ($Tag) {
  Write-Host "Tag          : $Tag"
} else {
  Write-Host "Bump         : $($levels[$bump])"
  Write-Host "Next version : $version"
}

if ($env:GITHUB_OUTPUT) {
  "version=$version"   | Out-File $env:GITHUB_OUTPUT -Append -Encoding utf8
  "tag=v$version"      | Out-File $env:GITHUB_OUTPUT -Append -Encoding utf8
  "previous=$previous" | Out-File $env:GITHUB_OUTPUT -Append -Encoding utf8
}
exit 0
