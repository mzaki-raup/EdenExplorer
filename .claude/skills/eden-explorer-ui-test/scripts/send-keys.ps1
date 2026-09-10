param(
    [Parameter(Mandatory=$true)][string]$Keys
)

# Sends keystrokes via .NET SendKeys - use this (not click.ps1) for anything
# involving modifier keys (Ctrl/Shift/Alt combos) or for "type text" /
# {ENTER} / {ESC} / {DELETE} style keys. SendKeys syntax: modifiers are
# prefixes - "^" Ctrl, "+" Shift, "%" Alt, e.g. "^v" (Ctrl+V), "+{DELETE}"
# (Shift+Delete), "^+c" (Ctrl+Shift+C). Special keys go in {BRACES}: {ENTER},
# {ESC}, {DELETE}, {TAB}, {F2}.
#
# This always force-focuses the window first (see focus.ps1) because
# SendKeys delivers to whatever window has *real* OS focus - not whatever
# click.ps1 last posted a message to. Skipping that step is the single most
# common cause of "I sent the keys but nothing happened" (they silently went
# to a different window, e.g. back to the agent's own terminal/app).
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$focusOutput = & (Join-Path $scriptDir "focus.ps1")
if ($LASTEXITCODE -ne 0) {
    Write-Output "ABORTED: could not focus EdenExplorer, so these keys were NOT sent (they would have gone to the wrong window): $focusOutput"
    exit 1
}

Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.SendKeys]::SendWait($Keys)
Write-Output "sent: $Keys"
