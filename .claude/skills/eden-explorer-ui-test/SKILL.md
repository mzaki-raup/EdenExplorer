---
name: eden-explorer-ui-test
description: Build, launch, and drive the actual running EdenExplorer.exe window to visually verify a UI/behavior change before calling it done - clicking, right-clicking, drag-selecting, and sending keyboard input (including modifier combos), plus screenshotting the result. Use this whenever a change touches anything the user sees or interacts with (context menus, column headers, rename/delete flows, preview pane, drag-drop, keyboard shortcuts) and you're about to report the change as working - "it compiles" is not the same as "it works," and this app's egui/Win32 layer has enough sharp edges (see Gotchas below) that a change can compile clean and still be wrong in practice. Also use it whenever the user asks you to "test", "verify", "check", "try", or "show" a change in the running app.
---

# EdenExplorer live UI testing

EdenExplorer is a Rust/egui Windows file manager (single-window, no browser,
no accessibility tree exposed) - the only way to know a UI change actually
works is to build it, click on it, and look at a screenshot. This skill
packages the input-simulation approach that was worked out the hard way
(a lot of trial and error) into scripts under `scripts/`, plus the gotchas
that cost the most time so you don't rediscover them.

## Workflow

1. **Build and launch**: `scripts/build-and-launch.ps1` (defaults to
   `-Profile release`). It stops any already-running instance first, because
   a running `EdenExplorer.exe` locks its own file and `cargo build --release`
   fails with "Access is denied" otherwise.
2. **Set up an isolated test scenario.** Never test against the user's real
   files - see **Safety** below. Create a scratch folder under `D:\` (or
   wherever this machine's non-system drive is) with a couple of throwaway
   files, e.g. `mkdir /d/eden-test && echo hi > /d/eden-test/a.txt`.
3. **Navigate the app to that folder.** `scripts/focus.ps1` then click the
   address bar breadcrumb to enter edit mode, then `scripts/send-keys.ps1
   -Keys "^aD:\eden-test{ENTER}"` (Ctrl+A selects the existing breadcrumb
   text, then typing replaces it, matching how the address bar's edit mode
   behaves).
4. **Drive the interaction** with `click.ps1` / `drag.ps1` / `send-keys.ps1`
   as needed, taking a `screenshot.ps1` after each meaningful step and
   reading it back with the Read tool - don't assume what happened, look.
5. **Verify, then clean up**: `rm -rf` the scratch folder when done. Don't
   leave test artifacts (files, tag groups, favorites, tab groups) behind in
   the app's persisted state either - if a step created one as a side
   effect, remove it the same way a user would (through the UI, or by
   editing the relevant `%LOCALAPPDATA%\ExplorerEden\*.bin` file only if
   the UI can't).

Coordinates for `click.ps1`/`drag.ps1` are **client coordinates** (origin at
the window's own top-left) - the same frame a `screenshot.ps1` image is in,
so line them up directly against what you see in the screenshot.

## Safety - read before you start clicking

This app's sidebar/favorites default to the user's real `Downloads`,
`Documents`, etc. It is very easy to end up looking at (or worse,
right-clicking / deleting / copying) the user's actual files by accident -
this has happened multiple times during development (a real credentials
file's contents nearly ended up on the clipboard from an over-broad click).
Before every click, know which folder you're in and whether it's your
scratch folder or a real one:

- **Never** select, open, delete, rename, or copy-to-clipboard anything
  outside a scratch folder you created for this test.
- If a screenshot surprises you by showing a real user folder (sidebar
  favorites, "This PC", a stray tab that survived from a previous session),
  stop and navigate back to your scratch folder before continuing - don't
  "just quickly" click through it.
- If you're testing a clipboard-copy feature, `Set-Clipboard -Value
  'marker'` first so you can tell real content copied vs. nothing happening
  vs. stale clipboard content from before.
- Clean up scratch files/folders when you're done, same as any other
  temporary test artifact.

## Gotchas (each one cost real debugging time - read before assuming a "no effect" result means the feature is broken)

- **`SetForegroundWindow` alone is silently ignored.** A process that wasn't
  itself the last-focused app can't just steal focus - Windows blocks it.
  `focus.ps1` uses the `AttachThreadInput` workaround; use it before
  anything keyboard-related, and re-run it if focus may have shifted (e.g.
  after alt-tab-like side effects from other automation).
- **Calling `focus.ps1` and then `click.ps1`/`send-keys.ps1` as two
  *separate* Bash tool calls can silently lose the focus `focus.ps1` just
  won - each call spawns its own fresh PowerShell process, and that new
  process itself briefly becomes the real foreground window the instant it
  starts, stealing focus back before the click/keystroke is posted.**
  Confirmed directly once: `focus.ps1` printed success, but polling
  `GetForegroundWindow()` immediately after showed a `powershell.exe`
  process (this very tooling) in the foreground, not EdenExplorer - every
  click at that point posted to the right window handle but had no
  effect. The fix: chain `focus.ps1` and the click/send-keys call in one
  PowerShell invocation so no new process gets a chance to steal focus in
  between:
  ```bash
  powershell -Command "
  & '.claude/skills/eden-explorer-ui-test/scripts/focus.ps1'
  & '.claude/skills/eden-explorer-ui-test/scripts/click.ps1' -X 100 -Y 200
  "
  ```
  If clicks or keystrokes stop registering mid-session despite `focus.ps1`
  reporting success, try this before spending time on other workarounds.
- **`SendKeys` goes to whatever has *real* OS focus** - not whatever
  `click.ps1` last posted a message to (`PostMessage` doesn't change actual
  focus). `send-keys.ps1` calls `focus.ps1` internally for exactly this
  reason; if you roll your own keystroke sending, don't skip that step or
  the keys may silently land on a different window (a background terminal,
  the agent's own app, etc.) with no error.
- **`PostMessage`-based double-clicks are unreliable.** Two `click.ps1`
  calls in a row often don't register as a double-click in egui, regardless
  of the delay between them. To "open" an item, select it with a single
  click and then `send-keys.ps1 -Keys "{ENTER}"` instead.
- **`PostMessage` clicks don't update the real OS modifier-key state**, so
  Ctrl/Shift/Alt-held clicks look like plain clicks to egui. For anything
  needing a held modifier, use `send-keys.ps1`'s modifier syntax
  (`+{DELETE}` for Shift+Delete, `^v` for Ctrl+V) rather than `click.ps1`.
  Also worth knowing: even a *real*, correctly-modified keypress can fail to
  register as expected in this specific app (Shift+Delete's modifier has
  been observed not reaching the delete handler in some environments) - if
  a modifier-combo shortcut seems not to fire, that may be a genuine app
  gap, not a test-harness problem; check whether an equivalent context-menu
  action exists as a fallback before concluding the feature itself is
  broken.
- **A real Windows secure-desktop prompt (UAC, Windows Hello) blocks ALL
  synthetic input system-wide** and can't be dismissed or interacted with
  programmatically by any process. If one appears, stop and tell the user -
  don't try to work around it.
- **Debug vs. release builds differ in whether you can see `eprintln!`
  output.** `src/main.rs` sets `windows_subsystem = "windows"` only under
  `cfg(not(debug_assertions))`, so a release build has no console at all.
  For debug logging, temporarily add `eprintln!`, build with
  `build-and-launch.ps1 -Profile debug`, and launch via `Start-Process
  -RedirectStandardOutput <path> -RedirectStandardError <path>` so you can
  read the output with the Read tool afterward. Remove the debug logging
  before considering the change finished - it's a diagnostic aid, not
  something to ship.
- **egui's built-in text selection (`Label`/`TextEdit`) collapses to the
  click point on *any* pointer button press, including right-click**,
  before the widget reacts to that click. If you're testing a "select text,
  then right-click to copy" flow, don't be surprised that the selection
  visually collapses right after the copy fires - that's expected, not a
  bug (see the `draw_selectable_text_with_copy` / `arm_markdown_copy_on_right_click`
  doc comments in `itemviewer_preview.rs` for the full reasoning if you need
  to touch that code again).
