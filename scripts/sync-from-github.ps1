<#
.SYNOPSIS
  Brings commits made on the GitHub copy of this repository back to the
  Forgejo instance that owns it.

.DESCRIPTION
  The two forges share one history: Forgejo is where releases are cut, GitHub
  is a second place people can read, download, and contribute. Nothing syncs
  GitHub -> Forgejo on its own (a Forgejo pull mirror would make this repo
  read-only, and a push mirror force-pushes over anything GitHub has), so this
  is the one deliberate step: fetch GitHub's main, fast-forward the local main
  onto it, and push the result to Forgejo - whose release workflow then does
  what it always does, including pushing the resulting tag back to GitHub.

  Fast-forward only. If both sides have moved, this stops and says so rather
  than merging on your behalf: resolve it by hand (git merge github/main),
  then run it again.

  Expects a remote called "github" (see README.md, "Two forges").
#>
param(
  [string]$Branch = "main",
  [string]$GitHubRemote = "github",
  [string]$ForgejoRemote = "origin"
)

$ErrorActionPreference = "Stop"

function Invoke-Git {
  git @args
  if ($LASTEXITCODE -ne 0) { throw "git $($args -join ' ') failed with exit code $LASTEXITCODE." }
}

if (-not (git remote | Where-Object { $_ -eq $GitHubRemote })) {
  throw "No remote named '$GitHubRemote'. Add it: git remote add $GitHubRemote https://github.com/<user>/<repo>.git"
}

if (git status --porcelain) {
  throw "The working tree has uncommitted changes - commit or stash them first."
}

Invoke-Git fetch --tags $ForgejoRemote
Invoke-Git fetch $GitHubRemote

$current = (git rev-parse --abbrev-ref HEAD).Trim()
if ($current -ne $Branch) { Invoke-Git checkout $Branch }

# Local main must already carry Forgejo's main (the release workflow commits
# version bumps there), or the fast-forward below would push a stale branch.
Invoke-Git merge --ff-only "$ForgejoRemote/$Branch"

$ahead = (git rev-list --count "$Branch..$GitHubRemote/$Branch").Trim()
if ($ahead -eq "0") {
  Write-Host "Nothing on $GitHubRemote/$Branch that $Branch does not already have."
  exit 0
}

Write-Host "$ahead commit(s) on GitHub to bring over:"
git log --oneline "$Branch..$GitHubRemote/$Branch"

git merge --ff-only "$GitHubRemote/$Branch"
if ($LASTEXITCODE -ne 0) {
  throw "Both forges have moved since they last agreed. Merge by hand (git merge $GitHubRemote/$Branch), then run this again."
}

Invoke-Git push $ForgejoRemote $Branch
Write-Host "Pushed $Branch to $ForgejoRemote - its release workflow takes it from here."
exit 0
