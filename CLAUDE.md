# EdenExplorer

A Rust/egui Windows file manager (Windows 10/11) - a fast, open-source
alternative to Windows File Explorer. Single native window, no browser
runtime, no accessibility tree: every UI surface (Details/Columns/Gallery/
Preview views, Recycle Bin, tag groups, settings) is hand-drawn by egui each
frame. Windows shell integration (delete/recycle, context menus, file
properties, drive info, recycle bin enumeration) goes through `windows-rs`
Win32 bindings, not a higher-level abstraction.

## Project skills

Four skills under `.claude/skills/` capture project-specific knowledge that
would otherwise get re-derived (expensively) every session:

- **eden-explorer-ui-test** - build, launch, and drive the running app
  (click/drag/keyboard/screenshot) to verify a UI change actually works.
- **egui-patterns-reference** - non-obvious egui techniques already worked
  out here (column auto-fit/reorder, selectable read-only text,
  right-click-copy timing, the click-to-rename gesture) - consult before
  re-implementing something that rhymes with one of these.
- **egui-design-review** - a consistency/polish checklist (spacing, theme
  colors in both light/dark, icons, interaction parity, i18n) to run before
  calling a visual change done.
- **i18n-sync-check** - a script that catches i18n keys missing from any of
  the six locale files - run it after touching any `i18n.tr(...)` call or
  locale file.

## Build & run

```bash
cargo build --release          # target/release/EdenExplorer.exe
cargo build                    # debug build - keeps its console attached (see Gotchas)
cargo check                    # fast type-check without codegen
```

A running `EdenExplorer.exe` locks its own file, so a release build fails
with "Access is denied" if an old instance is still open - stop it first
(`Stop-Process -Name EdenExplorer -Force`) or use
`.claude/skills/eden-explorer-ui-test/scripts/build-and-launch.ps1`, which
does that automatically.

There is no automated test suite - correctness for UI-facing changes is
verified by actually running the app. See **Verifying a change** below.

## Architecture map

- `src/main.rs` - entry point; sets `windows_subsystem = "windows"` only
  under `cfg(not(debug_assertions))` (see Gotchas: console visibility).
- `src/core/` - platform/business logic with no egui dependency: `fs.rs`
  (file listing, `FileItem`), `indexer.rs` (settings/tags/favorites
  persistence, all as separate small files under
  `%LOCALAPPDATA%\ExplorerEden\`), `drives.rs`, `preview.rs` (text/markdown/
  image/PDF/archive preview payloads), `video.rs`, `context_menu_settings.rs`
  (user-defined custom right-click commands), `tab_groups.rs`, `network.rs`,
  `portable.rs`, `launch.rs`.
- `src/gui/windows/mainwindow_imp.rs` - the big one: native Win32 file
  operations (`IFileOperation`/`IShellItem` COM calls for copy/move/delete/
  rename with real Explorer-style progress and confirmation dialogs), and
  the central `match` that handles every `ItemViewerAction` /
  `ItemViewerContextAction` emitted by the views below.
- `src/gui/windows/containers/` - the views and their shared state:
  - `itemviewer.rs` - Details/DetailPreview table view (the main file list)
  - `itemviewer_columns.rs` - Miller-columns (Finder-style) view
  - `itemviewer_gallery.rs` - thumbnail grid view
  - `itemviewer_preview.rs` - Preview-layout's file list + content preview
    pane (text/markdown/image/video/archive rendering)
  - `itemviewer_helper.rs` - shared logic used by multiple views: column
    layout/auto-fit/reordering, rename-gesture state machine, context
    menus, keyboard shortcut handling
  - `itemviewer_navbar.rs`, `sidebar.rs`, `tabs.rs`, `tags.rs`, `topbar.rs`
  - `enums.rs` / `structs.rs` - the shared `ItemViewerAction`/
    `ItemViewerContextAction` vocabulary and per-view state structs
    (`ExplorerState`, column state, etc.)
- `src/gui/windows/settings.rs` + `*_ui.rs` - the Settings window's pages
  (General, Behavior, Appearance, Favorites, Custom Context Menu, Tab
  Groups, Tags, Advanced).
- `src/gui/i18n.rs` + `locales/<lang>/main.ftl` - Fluent-based i18n; see
  **i18n** below before adding any user-facing string.
- `src/gui/windows/windowsoverrides.rs` - low-level window-mode/DWM/
  foreground-thread helpers, including the global `egui::Context` stash
  (`set_egui_ctx`/`request_repaint()`) background threads use to wake the UI
  without threading a `Context` through every call signature.

## Conventions

- **Comments**: sparse, and only to explain a non-obvious *why* (a hidden
  constraint, a workaround, a subtlety a future reader would trip over) -
  not what the code does. This file and the codebase both follow that; match
  it rather than adding narrating comments.
- **No speculative abstraction.** Three similar call sites are fine as three
  call sites. Don't introduce a trait/generic/config flag for a case that
  doesn't exist yet.
- **egui table columns** (Details view, Recycle Bin, the per-tag-group
  table in `tags.rs`) follow one consistent pattern: an explicit
  `ColumnState`-style struct holding `order: Vec<Column>` + per-column
  widths, a `pending_fit_request` + `layout_generation: u64` pair where
  bumping the generation and folding it into the `TableBuilder`'s `id_salt`
  forces egui to drop its own persisted column-width memory and re-read the
  `initial()` values you just computed - this is the only reliable way to
  make "size column(s) to fit" and drag-to-reorder work, since egui's
  `Column::initial()` is only consulted on a table id's *first* render.
  Follow this pattern (see `ItemViewerColumnState` in `structs.rs`, or
  `TagColumnState` for the smaller per-tag-table version) rather than
  inventing a new one for a new table.
- **Auto-fit-once-on-first-open**: column widths auto-fit to content the
  first time a view renders with real data in it (compare current widths
  against the type's `Default`), not on every navigation - so a user's
  manual resize sticks. Render-time-driven (checked every render, gated by
  an `auto_fit_checked` bool) rather than hooked into a navigation event, so
  it fires correctly regardless of how the view got loaded (first tab on
  startup, switching to an already-open tab, a new tab, a split pane).
- **i18n**: every user-facing string goes through `i18n.tr("key")`, backed
  by Fluent `.ftl` files. When adding a string, add the key to **all six**
  `locales/<lang>/main.ftl` files (`en-US`, `id-ID`, `ja-JP`, `zh-CN`,
  `zh-HK`, `zh-TW`) - a key missing from a non-English file typically just
  falls back silently, so it's easy to forget and ship an untranslated key
  without any build error to catch it. Reuse an existing key (e.g.
  `explorer_cols_*`, `recycle_bin_delete_permanently`) instead of adding a
  near-duplicate when the English text is already identical or close enough.
- **Settings/state persistence**: favorites, tags, tab groups, custom
  context menu entries, and window/session state are each persisted as
  their own small file under `%LOCALAPPDATA%\ExplorerEden\` (see
  `core::indexer`), not bundled into one big settings blob - keeps unrelated
  features from needing to touch a shared positional struct/tuple when only
  one of them changes.
- **Native file operations** (delete/copy/move/rename) go through
  `IFileOperation` (see `delete_paths_native` and friends in
  `mainwindow_imp.rs`), which is what gives real Explorer-style progress
  dialogs and the native "are you sure?" confirmations
  (`FOF_WANTNUKEWARNING`) - don't drop to a manual `std::fs::remove_file`
  path for anything user-facing.

## Verifying a UI change

Compiling clean is not sufficient for anything touching what a user sees or
clicks - use the **eden-explorer-ui-test** skill
(`.claude/skills/eden-explorer-ui-test/`) to actually build, launch, click
through, and screenshot the running app before calling a UI/behavior change
done. It documents the specific gotchas around this app's input-simulation
(focus-stealing, unreliable synthetic double-clicks, modifier keys, debug
vs. release console visibility) that have each cost real debugging time
before - read it rather than rediscovering them.

## Safety rules (non-negotiable)

- **Never touch the user's real files during testing.** The sidebar/
  favorites default to real `Downloads`/`Documents`/etc., and it is easy to
  end up looking at, selecting, or (worse) deleting/copying real user data
  by accident while driving the app for a test - this has actually happened
  during development (a real credentials file's contents nearly ended up on
  the clipboard from an over-broad click). Always create an isolated
  scratch folder for live testing, verify you're in it before interacting,
  and delete it when done.
- **Never leave debug scaffolding behind.** If you add a temporary
  `eprintln!`/`println!` to diagnose something, remove it before considering
  the change finished - it's a diagnostic aid, not something to ship.
- **Don't touch git remotes, force-push, or amend published commits**
  without being explicitly asked. `origin` here is the maintainer's own
  server (`https://git.zacx.net/owner/eden-explorer.git`) on branch `main` -
  note that this is *not* the public GitHub repo the README links to, so
  don't assume a push goes to GitHub or that GitHub-specific tooling
  (`gh`, PR commands) applies to this checkout.

## Gotchas worth knowing before you hit them

- **Shift+Delete's modifier can fail to reach the delete handler** in some
  environments (confirmed via direct testing: `i.modifiers.shift` read as
  unset at the moment the Delete keypress was processed, despite the
  physical key genuinely being held). The keyboard shortcut code path is
  still there and may work fine for you, but don't rely on it alone - the
  right-click context menu's **Delete Permanently** entry is the
  guaranteed-reliable path and exists specifically because of this.
- **egui's built-in text selection collapses to the click point on *any*
  pointer button press, including right-click**, before the widget reacts to
  that click - there is no public API to read "the selection as it was a
  moment ago" after the fact. The preview pane's "select text, right-click
  to copy" feature (`itemviewer_preview.rs`) works around this by capturing
  the selection from a `TextEdit`'s persisted state *before* the press is
  processed each frame; read the doc comments on
  `draw_selectable_text_with_copy` / `arm_markdown_copy_on_right_click`
  before changing anything there, the ordering is load-bearing.
- **Debug builds show a console** (for `eprintln!`/`println!`), release
  builds don't (`windows_subsystem = "windows"` under
  `cfg(not(debug_assertions))` in `main.rs`). If you need to see debug
  output, build debug and redirect stdout/stderr when launching rather than
  expecting a console window to just appear.
- **`egui::TextEdit::frame(...)` takes a `Frame`, not a `bool`, in this
  egui version.** `.frame(false)` (the pattern you'll find in older egui
  examples/docs online) fails to compile here - use `.frame(egui::Frame::NONE)`
  to get a borderless/backgroundless text widget instead. A version mismatch
  like this won't be obvious until you actually try to build; don't assume
  an API shape from memory or from a different egui version's docs when the
  compiler can just tell you.

## Mistakes already made once here - don't repeat them

Everything in this section actually happened during development and cost
real time to track down. Most are about the *live-testing workflow*
specifically (driving the real running app via synthetic input, from a
PowerShell process) rather than the app's own code - if you're using the
`eden-explorer-ui-test` skill, read this before you start.

- **A "no visible effect" during live testing can mean your input went to
  the wrong window, not that the feature is broken.** More than once, a
  click/keystroke that appeared to do nothing turned out to have been
  correctly processed by *some* window - just not EdenExplorer, because
  real OS focus had silently ended up elsewhere (see the next two points).
  Before concluding a feature doesn't work from a "nothing happened"
  result, verify `GetForegroundWindow()` actually is EdenExplorer's window
  (the `eden-explorer-ui-test` skill's `focus.ps1`/`screenshot.ps1` now
  check this automatically and print a warning instead of failing silently
  - but if you're not using those scripts, check it yourself).
- **A screenshot taken via `CopyFromScreen` captures whatever is on screen
  at those coordinates, not necessarily the target window's own content.**
  If another window (the agent's own terminal/app) is occluding
  EdenExplorer at the same screen position - which can happen even when
  EdenExplorer is nominally "focused" - the screenshot silently shows the
  wrong window with no error at all. This actually happened: a screenshot
  meant to show EdenExplorer showed the Claude Code app's own conversation
  view instead, and it looked completely plausible as a normal screenshot
  until read carefully. Always verify the foreground window before trusting
  a screenshot (see previous point), and actually look at what a screenshot
  shows rather than assuming it worked because the capture command exited
  cleanly.
- **`SetForegroundWindow` alone is silently ignored** when called from a
  process that wasn't itself the last-focused app (Windows' "foreground
  lock") - it returns success and does nothing. Needs the
  `AttachThreadInput` workaround (see `focus.ps1` in the ui-test skill).
- **PowerShell script exit codes: a script that completes without an
  explicit `exit 0` leaves `$LASTEXITCODE` as `$null`, not `0`, when called
  via `&` from another script.** `if ($LASTEXITCODE -ne 0)` then evaluates
  `$null -ne 0`, which is `$true` in PowerShell - a script that actually
  *succeeded* gets treated as a failure by its caller. Always give a
  PowerShell script an explicit `exit 0` on its success path if anything
  might check its exit code, not just `exit 1`/`exit N` on failure paths.
- **PowerShell has a built-in alias `lp` for `Out-Printer`.** A helper
  function named `LP` (e.g. for building a `WM_MOUSEMOVE`/click `lParam`)
  collides with it - PowerShell resolved calls to the intended function as
  the `Out-Printer` alias instead, failing with a confusing
  `Out-Printer : A positional parameter cannot be found...` error that
  doesn't obviously point at the real cause. Avoid short/generic PowerShell
  helper-function names; something like `MakeLParam` doesn't collide with
  anything built in.
- **`Set-Clipboard -Value ''` throws `ArgumentNullException`** - PowerShell
  treats an empty string as null here. Use a single space (`' '`) instead
  when you want to clear the clipboard to something inert (e.g. after
  testing a copy-to-clipboard feature).
- **Invoking PowerShell from a POSIX shell (Git Bash / the Bash tool):
  don't let the outer shell expand `$env:VAR`-style PowerShell syntax.**
  `powershell -Command "... $env:TEMP ..."` run through Bash gets `$env`
  expanded by *Bash* first (to nothing, since Bash has no such variable),
  silently mangling the path before PowerShell ever sees it. Either pass
  PowerShell environment-variable references inside a `-File script.ps1`
  (where PowerShell itself does the expansion, not Bash) rather than inline
  on a Bash-constructed command line, or use single quotes / an
  already-resolved literal path when calling from Bash.
- **This checkout trips git's "dubious ownership" safety check** (the
  `.git` directory is owned by `BUILTIN/Administrators`, not the running
  user), so `git status`/`git log`/etc. fail immediately with a fatal error.
  Git's own error message suggests
  `git config --global --add safe.directory ...`, but don't run that - it's
  a persistent global config change made on the user's behalf. Prefer the
  per-command form, which is equivalent for reads and writes and leaves no
  trace:
  ```bash
  git -c safe.directory=D:/WORKSPACES/RUST/eden-explorer status
  ```
  (Verified working for `status`, `log`, `add`, `commit`, and `push`.)
- **Automated/synthetic clicks can land somewhere unintended and create
  real side effects** (a stray tag group, a folder added to Favorites, a
  file selected in a real - not scratch - folder) if a coordinate assumption
  from a previous screenshot turns out stale (tab layout shifted, a popup
  was still open, focus moved). Screenshot and actually look before firing
  off a rapid sequence of clicks based on old coordinates, and clean up
  anything an errant click created - don't assume a "harmless" wrong click
  had no effect just because it wasn't the one you meant to make.
- **Real double-clicks and modifier-held clicks can't be reliably
  synthesized with `PostMessage`** - see the `eden-explorer-ui-test` skill's
  Gotchas section for the two working alternatives (select-then-`{ENTER}`,
  and `SendKeys` for modifier combos) rather than spending time trying to
  make `PostMessage` double-clicks reliable; that path has already been
  tried and doesn't work.
- **A hardcoded fudge-factor added to one sibling's requested height in a
  `left_to_right` layout inflates the whole row - and everything nested
  below it - because egui lets a `Ui`'s `max_rect` auto-grow to fit an
  oversized `allocate_ui_with_layout` request.** In
  `mainwindow.rs`, the sidebar column was allocated with
  `ui.available_height() + 15.5` (an old visual tweak, cause unknown/undocumented). Because
  the sidebar and the explorer column are siblings in one `left_to_right`
  row that fills the entire central panel, that `+15.5` grew the row's own
  `max_rect` by ~15px past the real window bottom, and that inflated bound
  then propagated all the way down through the split-pane rect computation
  (`split_rect`/`primary_rect`/`secondary_rect`) into each pane's
  `draw_tab_content`. The practical symptom: a newly added status-bar strip,
  sized as an exact small row pinned to the bottom via
  `StripBuilder::size(Size::exact(status_height))`, computed a `max_rect`
  that ended a few pixels *below the actual OS window's bottom edge* - so
  it painted into real, off-screen coordinates. Nothing rendered, not even
  a hardcoded-color fill on the topmost `Order::Foreground` layer, because
  the paint commands were genuinely outside the render surface, not merely
  clipped or z-ordered wrong. Existing scrollable/growable content never
  exposed this because a few extra px of phantom space at the bottom is
  invisible when nothing is pinned exactly to it. Diagnosed by comparing
  `ui.ctx().viewport_rect()` (the real window) against `ui.max_rect()` at
  each layout stage from `CentralPanel::show` downward via temporary
  `eprintln!`s, which showed the mismatch first appearing exactly at the
  sidebar's `allocate_ui_with_layout` call. Fixed by removing the `+ 15.5`.
  If a similar "nothing renders, even on the Foreground layer, even though
  the code executes every frame" symptom shows up again, compare
  `ui.max_rect()`/`ui.clip_rect()` against `ui.ctx().viewport_rect()` at
  each layout nesting level before assuming a z-order/clipping bug - a
  rect that's simply larger than the physical window won't render no matter
  what layer it's painted on.
- **Equal top/bottom `egui::Frame` inner margin does not produce equal
  visual whitespace around icon-font glyphs, because the glyph's own ink
  isn't vertically centered within its line-height box.** The status bar's
  Phosphor icons (`explorer.rs`, the footer `strip.cell`) were given an
  equal `STATUS_BAR_VPAD` margin top and bottom, expecting symmetric
  padding - but pixel-sampling a live screenshot showed a ~9px gap above
  the divider and only ~1px before the icons started rendering, i.e. most
  of the glyphs' line-height box is empty space *above* the ink, not below
  it. The fix was not to make the margin "more correct" some other way,
  but to deliberately use an *unequal* margin (`VPAD - 4` top, `VPAD + 4`
  bottom, tuned by re-sampling pixels after each change) so the visible
  gap comes out equal despite the glyph's own lopsided box. If another
  icon-adjacent row looks off-center despite seemingly-correct centering
  logic, sample actual pixel rows (a quick `Bitmap.GetPixel` loop over a
  screenshot, scanning for where the background color stops) rather than
  trusting `Align::Center`/equal margins to be visually equal - egui centers
  the glyph's box, not its ink.
- **A synthetic double-click meant to enter a sidebar test folder instead
  entered `%LOCALAPPDATA%\ExplorerEden` (the app's own real settings
  folder), and the paste that followed wrote 2000 scratch test files
  straight into it**, briefly mixing throwaway data in with
  `settings.bin`/`favorites/`/`tags.bin`/etc. This was caught immediately
  from the resulting screenshot (the breadcrumb read "...AppData > Local >
  ExplorerEden", not the intended path) and cleaned up by deleting only
  files matching the test data's own naming pattern
  (`^file[0-9]+\.bin$`, verified via `find -regex` to match exactly the
  2000 files created, and to exclude every real settings filename) -
  never a blind `rm -rf` of the whole folder. This is the exact class of
  mistake the Safety Rules section above already warns about (verify
  you're in an isolated scratch folder *before* interacting, because a
  stray click can land somewhere real) - the addition here is what to do
  *after* it happens anyway: don't panic-delete broadly, identify a
  pattern that provably matches only the injected data, dry-run it as a
  count/list first, and diff that count against what you actually
  created before deleting anything. Root cause was never fully pinned
  down (a sidebar item at the clicked coordinates apparently didn't match
  what the previous screenshot showed there), which is itself a
  restatement of the "stale coordinates from a previous screenshot"
  gotcha already documented above - it cost real risk here specifically
  because the destination that frame happened to be a sensitive,
  non-scratch path rather than another throwaway folder.

## Operational Instructions

You must strictly adhere to these guidelines during every task execution. Prioritize system safety, meticulous tracking, and verification.

### Workflow & Execution Rules
- **Phase Execution:** Implement the latest plan and design phase-by-phase. Follow the latest version of developer notes if they exist.
- **Pre-Verification:** You must always verify and test that the current phase is fully working before updating any checklist or moving forward.
- **Git Automation:** If a git repository exists, you must always commit and push the code/documentation to the server after completing a phase.
- **Mistake:** If any mistake encounter during development and fix, always update in CLAUDE.md file in appropriate section so that you will not make the same mistake twice.

### File Tracking & Documentation Rules
- **Checklist File:** 
  - Create a checklist file in the root folder if it does not exist.
  - Detail every phase and sub-phase. 
  - Track states strictly as: `Completed`, `In-Progress`, or `Pending`.
  - Update this file immediately after verifying a phase's success.
- **Change Log:** Update the Change Log file (if it exists) every time you add, delete, change, or fix code, implementation, or deployment.
- **README.md:** Update the Readme file (if it exists) whenever changes are made to features, requirements, cross-platform build commands, or restrictions.
- **Summary File:** Update the Summary file (if it exists) whenever any project asset or file is modified.

### Restrictions & Protected Files
- **DO NOT MODIFY** the file `Features_Issues_And_Changes.md`(if it exists). This is strictly for the user's personal notes.
- **CRITICAL:** While you must never edit `Features_Issues_And_Changes.md`(if it exists), you **must** stage, commit, and push it along with your git operations.