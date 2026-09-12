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
- **It happened again in the very next testing session, this time landing
  on a different real folder (`D:\test-delete`, one of the user's own
  pre-existing tabs) instead of the intended scratch folder.** Root cause
  this time: this app restores every tab from the last session on launch,
  so the tab strip is full of the user's own real tabs from the start -
  a coordinate-based click meant for "whatever tab is currently active"
  can land on a stale/unintended tab if a previous action shifted which
  tab was active, or if tab widths shifted enough to move a tab header
  out from under a previously-good coordinate. No data was lost (the
  delete this time actually failed with a native HRESULT error before
  anything was removed, confirmed both from the folder's untouched
  contents afterward and from the debug console output), but it's the
  same underlying gap as the entry above: coordinate-based navigation
  trusted without re-verifying between steps. Also surfaced a false
  negative in the ui-test skill's `focus.ps1`: it reported "EdenExplorer
  is not running" once, when the process was in fact still alive
  (confirmed via `tasklist`) - transient, but worth a re-check via
  `tasklist`/`Get-Process` before believing a "not running" result enough
  to declare the app crashed. **The actual, durable fix going forward:**
  before any destructive keystroke (Delete, a paste that might overwrite,
  etc.), take a screenshot and confirm both the active tab's breadcrumb
  path *and* that the item about to be acted on is actually selected -
  every other test in this file that did that caught problems immediately
  and safely; every incident happened on a step that skipped it.
- **The paste-conflict modal's first "Rename" implementation silently
  destroyed the very file it was supposed to preserve.** Robocopy always
  writes a copy under its *source* filename - it has no destination-rename
  option. The first cut pointed a rename job's `dest_dir` straight at the
  real target directory (same as every other job), meaning robocopy wrote
  the new copy under the *original* colliding name first - directly
  overwriting the pre-existing file - and only *then* renamed the fresh
  copy to `<name>-001`. Net effect: the old file's content was gone, and
  the new copy just ended up under a different name - "Rename" behaved
  exactly like "Replace" from the user's perspective, which is what got
  reported. Root-caused by reasoning through the job's actual robocopy
  invocation rather than trusting a live test (this one has since chained
  through unrelated navigation problems every time it's been attempted -
  see the entries above and below). Fixed by staging a rename job's copy
  in a fresh, guaranteed-empty subdirectory of the target dir instead - a
  freshly-invented folder name can never collide with the file being
  preserved - and only moving the finished copy out to its final `-001`
  name (removing the empty staging folder afterward) once robocopy has
  fully finished, meaning the original file is never opened for writing
  at any point. If another "conflict resolution" style feature gets added
  later, check for exactly this shape of bug: does a "keep both" or
  "rename" path *ever* let the copy engine target the same path the
  original occupies, even transiently mid-operation?
- **A `ScrollArea::vertical().max_height(N)` reserves the full `N` up
  front regardless of how much it actually ends up drawing inside it** -
  it does not shrink to fit a shorter list. The paste-conflict modal used
  a flat `max_height(100.0)` for a "which items collided" list that's
  usually 1-3 lines, which (combined with an `egui::Window` that has
  nothing else forcing its height) left a visible gap between the button
  row and the window's bottom edge for any list shorter than what 100px
  actually holds. Two separate things had to be fixed here, not one: an
  earlier `.fixed_size([w, h])` on the `Window` itself (forces exact
  height no matter what - removed, replaced with `.min_width` only so it
  can't look too narrow for a 1-item list), *and* the ScrollArea's own
  flat max_height (replaced with a height computed from the actual row
  count, capped at the same ceiling). Fixing only the `Window`'s sizing
  looked complete after a code read but the gap was still there, because
  it was actually two independent sources of unclaimed space stacking on
  top of each other - if a dialog still looks like it has dead space
  after removing a `fixed_size`, check every `ScrollArea`/fixed-height
  child inside it too, not just the `Window` call itself.
- **A background handler wired up only inside the "has items" branch of
  a view silently does nothing for the "folder is empty" branch, even
  when the empty-state placeholder is drawn in the exact same visual
  area.** The Details/Detail-Preview views' existing double-click-
  background-to-navigate-up feature lived entirely inside `if
  !visible_items_empty { ... }` (it's wired through
  `table_background_response`, which only makes sense once there's an
  actual table), so double-clicking the "This folder is empty" text did
  nothing at all - not a broken gesture, a code path that was never
  reached. When adding an interaction handler that's meant to apply
  "whenever this general area is shown", double check it isn't
  accidentally scoped to only one of that area's rendering branches
  (populated vs. empty, loading vs. loaded, etc.) - grep for where the
  early-return/placeholder branch is and confirm the handler's scope
  actually covers it, rather than assuming it does because they're drawn
  in the same physical space.
- **`egui::Window` remembers its rendered size across every future `show`
  call with the same title/Id within a session - it does not shrink back
  down on its own just because a later call's content got shorter.** The
  paste-conflict modal's "gap below the buttons" complaint survived two
  real, correct fixes (removing a forced `fixed_size`, then sizing the
  internal `ScrollArea` to actual row count instead of a flat height)
  because neither of those addressed this third, independent cause: the
  very first time that modal was ever shown with a longer collision list,
  egui cached that larger size, and every later, shorter list kept
  rendering at the old cached size regardless of how little content it
  actually had. The fix is `.auto_sized()` on the `Window` builder, which
  disables the size-memory lookup entirely and recomputes from content
  every frame. If a `Window`/`Area`'s size looks "stuck" at an old value
  despite content-driven sizing code looking correct, suspect this before
  spending more time on the sizing math itself - especially for a Window
  whose title (and therefore Id) is a fixed string reused across many
  shows with varying content.
- **A per-frame `if pointer_released { ... }` drop handler that only ever
  *starts* an action, and never explicitly ends the drag state that got
  it there, leaves the drag visually "stuck" until something unrelated
  happens to reset it.** Both the Details/Detail-Preview and Gallery
  views' internal drag-and-drop handling fired an `ItemViewerAction::
  MoveItems` on drop but never set `drag_state.active = false` afterward
  - since the ghost label and drop-target highlight are both driven
  straight off that flag, they stayed on screen through the whole move
  operation and beyond, until the user started a fresh drag (which resets
  it as a side effect of arming a new one) or happened to trigger one of
  the few other reset sites. Fixed by unconditionally clearing
  `drag_state.active` inside the `pointer_released` branch, regardless of
  whether a valid drop target was found. When adding a "this fires once
  when the gesture ends" handler, check explicitly whether the state that
  *triggered* the handler is also the state something else is still
  reading afterward (here, `drag_active` for the ghost) - firing the
  action is not the same as ending the gesture that produced it.
- **`.auto_sized()` on an `egui::Window` that contains a `ScrollArea`
  can make the window balloon to nearly the size of the whole app,
  rather than shrinking to content.** This was the very next thing tried
  for the paste-conflict modal's "remembered size" problem (see the entry
  above) and made it dramatically worse: an auto-sized Window measures
  content against a close-to-unbounded available rect on its sizing pass,
  and the modal's internal `ScrollArea::vertical().max_height(...)`
  responded to that by claiming a large slice of it instead of respecting
  its own cap. The actual fix ended up not needing `.auto_sized()` at
  all: the conflict list is capped at 6 rows by construction (5 names +
  a "+N more" line) so it never genuinely needs to scroll, and dropping
  the `ScrollArea` entirely removed the interaction that caused the
  blowup. The "remembered size" problem itself was solved separately, by
  folding the row count into the Window's `Id` (`.id(Id::new(...)
  .with(row_count))`) instead of relying on `.auto_sized()` - a
  differently-sized conflict list is treated as a distinct Id with no
  stale remembered size, while a same-sized one correctly reuses the
  cached size (which needed to be that size anyway). If a `Window` needs
  to resize itself as its content's *row count* changes, prefer keying
  its `Id` off that count over reaching for `.auto_sized()` - especially
  if anything inside it is a `ScrollArea`.
- **Keying an `egui::Window`'s `Id` off a value that was already bad once
  doesn't fix it - `egui::Window`'s remembered-size cache is keyed by Id
  forever, per running session, and a fresh Id-generation *scheme* doesn't
  help if the specific Id it produces for the current content already has
  a bad size cached under it from before the scheme changed.** The
  paste-conflict modal's "gap below the buttons" bug survived three
  separate fix attempts (removing a forced `fixed_size`, a dynamic-height
  `ScrollArea`, `.auto_sized()`, then keying `.id()` by row count - see
  the entries above) because each one still used `egui::Window`, and the
  user kept reporting the exact same gap after every one of them ("still
  not fix!!"). The actual fix was to stop using `Window` for this modal
  entirely and rebuild it on `egui::Area` + `egui::Frame::popup` (the same
  primitive already used for the hamburger menu and the notifications
  panel elsewhere in this codebase) - `Area` lays out fresh from content
  every frame with no persisted size at all, so there is no cache to go
  stale in the first place. If a `Window`'s size is misbehaving and a
  couple of targeted fixes haven't stuck, don't reach for a fourth
  Id-keying variation - switch to `Area` instead of continuing to fight
  `Window`'s size memory from a new angle. Also worth remembering for next
  time: after telling the user something is fixed, verify it live with an
  actual fresh screenshot before saying so again - repeating "should be
  fixed" based on code review alone (without re-testing) is what turned a
  three-line bug into a session-long trust problem.
- **Clearing `drag_state.active` on drop isn't enough by itself if
  `drag_state.start_pos` is left set - a stale `start_pos` re-arms the
  drag on the very next frame.** The row/tile-level "has the drag exceeded
  a 4px threshold" check (`itemviewer.rs` and `itemviewer_gallery.rs`,
  near the `drag_started()` handling) only compares `start_pos` to the
  pointer's current `hover_pos()` - it never checks whether the mouse
  button is actually still down. The drop-completion reset (see the
  "ghost/highlight stayed on screen" entry above) only cleared `active`,
  not `start_pos` or `source_items`. Once the pointer's `hover_pos()` next
  drifted more than 4px from that leftover `start_pos` - which happens on
  essentially any subsequent mouse movement, drag or not - `active` got
  flipped back to `true` with no real drag in progress, and the ghost/
  highlight reappeared and then stuck permanently (each further pointer
  move just re-armed it again). This is exactly why the user's report
  ("still there... cursor not refresh to default state") survived what
  looked like a correct fix in the code. The Escape-cancel handler in
  `itemviewer_helper.rs` already cleared all three fields
  (`active`/`source_items`/`start_pos`) correctly - it was the reference
  pattern that the drop-completion reset should have matched from the
  start. When resetting a gesture-tracking struct with multiple fields
  that jointly define "is a drag in progress", clear *all* of them at
  every exit point, not just the one field a `.active`-style check happens
  to read - grep for every other place that reads the struct's fields to
  make sure none of them can revive it from a half-cleared state.
- **`ItemViewerAction::MoveItems` (drag-and-drop within the app) called
  `IFileOperation::MoveItem` directly instead of going through the same
  conflict-check the clipboard-paste path already had.** This meant a
  drag-and-drop onto a folder containing a same-named item popped a raw
  native Windows "confirm replace" dialog - differently styled from the
  app's own paste-conflict modal and without its Replace/Skip/Rename
  choices - and dropping an item onto the empty background of the folder
  it was already in (an easy accidental gesture, since the background
  drop-target fallback resolves to `current_dir` on any release over empty
  space) resolved `target_dir` to that item's own parent and triggered
  `IFileOperation`'s "the source and destination filenames are the same"
  error dialog for what should have been a silent no-op. Fixed by routing
  `MoveItems` through the exact same conflict-detection +
  `start_robocopy_paste` pipeline `paste_clipboard_native` already used
  (setting `pending_paste_conflict` on a collision, letting
  `handle_paste_conflict_resolution` do the rest), and by filtering out
  any source whose `parent()` already equals `target_dir` before doing
  anything else. When a codebase has two separate code paths that both
  ultimately mean "move/copy these items into this folder" (clipboard
  paste vs. drag-and-drop), check whether one of them quietly skipped
  work the other one already does correctly (conflict detection, safe
  rename-on-collision staging, self-move filtering) rather than assuming
  parity because the end result (files moved) looks the same.
- **`GetClientRect` returns physical pixels; egui's `i.viewport().inner_rect`
  and `ViewportBuilder::with_inner_size`/`with_position` all work in
  logical points - mixing the two silently breaks on any display that
  isn't at 100% scaling, and is invisible at 100%.** The window-size-on-
  exit-of-drag hook (`save_manual_window_size` in `windowsoverrides.rs`,
  fired on `WM_EXITSIZEMOVE`) saved `GetClientRect`'s raw physical-pixel
  width/height into the same `WindowSizeMode::Custom` field that the
  per-frame save in `mainwindow.rs` (correctly, from `i.viewport().
  inner_rect`) also writes to. On a scaled display (125%/150% is a common
  Windows default, especially on laptops) this silently inflated the
  remembered size by the scale factor; reloaded through `.with_inner_size`
  as if it were logical points, the requested window ended up larger than
  the physical monitor and got silently shrunk by `.with_clamp_size_to_
  monitor_size(true)` - which then throws off where the saved position
  ends up landing too. Invisible during dev/testing on a 100%-scaled
  machine, since physical and logical pixels are identical there. Fixed
  by dividing by `GetDpiForWindow(hwnd) as f32 / 96.0` before saving
  (the app's manifest already declares Per-Monitor-V2 DPI awareness -
  see `build.rs` - so this reads the real per-window DPI, not a stale
  system-wide value). When a Win32 call and an egui/eframe API both touch
  the same "pixels" value, check which pixel space each one actually
  means before assuming they're interchangeable - `GetClientRect`/
  `GetWindowRect`/`SetWindowPos` are physical; egui's own coordinate
  system (and anything built on `ViewportBuilder`) is logical points.
- **A fixed pixel budget subtracted from `ui.available_width()` for
  trailing controls (buttons, toggles) in a `ui.horizontal(...)` row
  overflows past the frame's own border once that row gets narrow enough**
  - a split pane, a smaller window, or (independently) a language whose
    translated button label is longer than the one the budget was tuned
    against. The search box's inline "This folder"/"Everywhere" scope
    toggle (`itemviewer_navbar.rs`, `draw_search_box_contents`) used
    `.desired_width(ui.available_width() - 160.0)` for the text edit,
    leaving a fixed 160px for the two trailing buttons - reasonable at the
    window width it was eyeballed against, but the buttons still spilled
    past the app's right border in a narrower window, because a plain
    `horizontal` layout doesn't wrap, it just keeps laying out past the
    available width. Fixed by switching to `ui.horizontal_wrapped(...)`
    (so trailing content wraps to a new line instead of overflowing) and
    giving the text edit its own fixed, modest `desired_width` (220.0)
    instead of "whatever's left after a guessed budget for the rest of the
    row." When a row mixes a growable text input with fixed-ish trailing
    controls whose total width isn't known in advance (translated text,
    variable row width), prefer `horizontal_wrapped` over subtracting a
    guessed constant from `available_width()`.
- **Reusing an existing view kind's column-width *storage* for a new view
  kind, without also matching its exact column *set*, silently writes one
  view's widths into another's persisted slots.** The search-results view
  reuses the "file view" `is_drive_view=false, is_recycle_bin_view=false`
  branch of `ItemViewerColumnState` for its column set (`visible_order` in
  `structs.rs`) so it could show an `OriginalDirectory` column - but
  `column_width`/`set_column_width`'s positional index maps for that
  branch are keyed to the *file* view's own five columns (Name/Type/Size/
  Modified/Created), which don't include `OriginalDirectory` and do
  include `Created` (which search doesn't show). Triggering "Size All
  Columns to Fit" on a search tab called `set_all_column_widths` with the
  same `(false, false)` pair a real folder tab uses, which routed the
  search results' content-derived Type/Size/Modified widths straight into
  the *global, persisted* `item_viewer_file_column_sizes` - the exact
  storage every normal folder tab reads its column widths from - because
  `set_column_width` writes by column name into whatever slot that
  `(is_drive_view, is_recycle_bin_view)` pair resolves to, with no concept
  of "this caller meant something different by that pair." Silently
  corrupting a real folder's remembered column widths from a search tab
  interaction is a much worse bug than the "not working" the user actually
  reported. Fixed by having `compute_item_viewer_column_layout` special-
  case search view at the very top: it now returns freshly content-fitted
  widths every single frame, never reads or writes any persisted storage,
  and never sets `column_sizes_changed` - trading away per-search manual
  resize/persistence (a search tab's columns are always auto-fit, which is
  what a "Size to Fit" click would have produced anyway) for the guarantee
  that a supplementary view can never touch another view kind's saved
  settings. When a new view kind reuses another one's `(is_drive_view,
  is_recycle_bin_view)`-keyed storage for column *selection*, check
  whether every write path keyed the same way (`set_column_width`,
  `set_all_column_widths`, the per-frame width-drift write-back) also gets
  called for that new view kind before assuming read-only reuse is safe -
  a column set that's a superset/subset of the reused branch's own set is
  exactly the mismatch that corrupts silently instead of erroring.
- **The built-in search fallback matched names with a plain substring
  `.contains()`, but Everything's own query syntax treats `*`/`?` as
  wildcards - so a query like `*.png` (which works fine against
  Everything) returned zero results from the built-in engine, since no
  real filename literally contains the three characters `*`, `.`, `p`...**
  Reported as "built-in search engine not working" for exactly this
  query. Fixed by adding `matches_search_query` (`core/fs.rs`): if the
  query contains no `*`/`?`, it's still a plain substring match (so
  typing a partial name works like the existing type-to-filter box
  always has); if it does, it's matched as a whole-name glob via a
  standard two-pointer wildcard algorithm instead. When a fallback/
  alternate engine is meant to be a drop-in replacement for another one,
  check that it actually accepts the *same query syntax*, not just that
  it returns "some" results for a plain keyword - a fallback that only
  handles the simple case isn't actually swappable, and the gap won't
  show up in testing unless the exact same query someone would type
  against the *other* engine is tried against this one too.
- **`postcard`'s binary format is purely positional - a new struct field
  must always be appended at the very end, never inserted alongside
  related fields in the middle.** While adding a `saved_searches: bool`
  flag to `SidebarSectionsSnapshot` (`core/indexer.rs`) - alongside the
  existing `places`/`storage`/`favorites`/`tags`/`shared_network` bools -
  the natural-looking edit was to insert it in that same group, before
  `shared_network`. Caught during review (not live, but worth recording
  since it's an easy mistake to make again): unlike JSON, postcard has no
  field names in the byte stream, so a field inserted mid-struct doesn't
  get skipped or defaulted for an old save file - it silently shifts every
  field that comes after it, misreading `shared_network`'s old byte as the
  new field and reading garbage/misaligned bytes for `shared_network`
  itself. `#[serde(default = ...)]` only rescues a field when the old
  byte stream runs out entirely (a genuinely *missing trailing* field) -
  it does nothing for a field inserted earlier in the struct. Fixed by
  appending `saved_searches` after `shared_network` instead. Whenever
  adding a field to any `Serialize`/`Deserialize` struct that's persisted
  via `postcard` (search this file's own `core::indexer` module for the
  existing `#[serde(default = ...)]` pattern - most persisted structs use
  it), add the new field at the literal end of the struct, never wherever
  it "reads best" alongside similar fields.
- **Mouse-click-based live UI testing (`click.ps1`) can go completely
  unresponsive for an entire session while keyboard-based testing
  (`send-keys.ps1`) keeps working perfectly** - a different failure mode
  from the previously-documented focus-stealing/input-queue-lag issues.
  While testing the Saved Searches feature, `focus.ps1` reported success
  (foreground window was genuinely EdenExplorer's), but repeated
  `click.ps1` calls against several different, unambiguous targets (a
  toolbar toggle button, a file row, a tab's close button, the theme
  toggle) produced zero visible effect across multiple retries and a
  fresh app relaunch - while in the same window, `send-keys.ps1` (Ctrl+F,
  typed text, Enter) worked exactly as expected and a real search
  executed correctly. Root cause not identified (possibly the user's own
  foreground application - a game - intercepting/absorbing posted mouse
  messages at the OS level despite `GetForegroundWindow` reporting
  EdenExplorer). When `click.ps1` produces no visible effect, don't
  assume the clicked feature is broken - first run a cheap, unrelated
  control click (e.g. a tab or a toolbar icon whose effect is obvious) to
  check whether clicks are working *at all* right now, and cross-check
  with a `send-keys.ps1`-only path if one exists for the feature being
  tested, before concluding anything about the code itself.
- **The mouse-click-unresponsive state above isn't a one-off - it recurred
  in a later session on the very same machine, persisting across a fresh
  release rebuild and app relaunch, while `Ctrl+A`/`Ctrl+F`/typed text/
  `Enter` all kept working perfectly the whole time.** This was hit again
  while trying to verify the Bulk Rename feature (which needs a
  multi-select + right-click to open). `Shift+F10` (Explorer's usual
  keyboard alternative to a right-click context menu) does **not** work as
  a substitute in this app - sending it did not open the context menu, and
  once even collapsed a 5-item selection down to 1 (something in this
  app's keyboard handling reacts to Shift+F10 in some other way, not
  "open context menu"). There is currently no verified keyboard-only path
  to a right-click context menu here. When this state recurs: confirm
  quickly with one cheap control click (a few retries, not a long
  investigation), then stop trying to force it and fall back to unit
  tests + careful code review for anything that specifically requires a
  mouse click (multi-select context menus chief among them) - don't burn
  significant time retrying `click.ps1` once it's confirmed broken this
  session, and don't try `Shift+F10` as a workaround, it's already been
  tried and doesn't work.
- **Found a real, working keyboard-only path around the mouse-click issue
  above, for anything with its own keyboard shortcut that's a superset of
  what a right-click/menu path does.** Bulk Rename can be opened via
  either the right-click "Rename" entry or the **F2** key (both routed to
  the same `BulkRenameRequest` action once multi-selection is detected) -
  so with the mouse dead, `Ctrl+A` (select all) then `{F2}` via
  `send-keys.ps1` opened the dialog perfectly, and once open,
  **`{TAB}` + a literal space (`" "`) reliably moves focus and activates
  checkboxes/buttons**, including committing a real batched rename and
  landing on the resulting completion toast. When a mouse-broken session
  needs to drive a feature, check first whether it has its own keyboard
  entry point (a shortcut key, an accelerator) before assuming it's
  untestable - don't stop at "no right-click" the way the Shift+F10 entry
  above did without checking for an alternate trigger.
- **A screenshot's visual "this widget is focused" cue (an outline, an
  underline) is not reliable enough to trust by itself when mapping a
  dialog's Tab order - verify each stop empirically by pressing a literal
  space and observing the actual effect, not by eyeballing which widget
  looks highlighted.** While Tab-navigating the Bulk Rename modal, a
  custom-drawn checkbox's row rendered with what looked like an underline
  on its *label text* across two different focus positions in a row,
  which read as "focus is stuck here" - but pressing space at each of
  those two positions produced two different real effects (the checkbox
  toggled once, then two tabs later the whole dialog closed via Cancel),
  proving the visual cue wasn't tracking focus reliably enough to count
  Tab stops by eye. The reliable method: after each `{TAB}`, press a
  space and check what actually changed (a checkbox toggled, a button's
  effect happened, nothing happened) rather than trying to read focus
  state from the screenshot.
- **A dedup check comparing two different path formats can silently never
  match, defeating the dedup entirely - always sanity-check that both
  sides of a `.contains()`/`HashSet` comparison are actually in the same
  format before trusting it.** The sidebar's "Storage" section listed
  every USB-attached disk *twice* - once correctly as its lettered volume
  (e.g. `D:\`), and again as a bogus "Non-NTFS Drive (\\.\PhysicalDriveN)"
  entry, even when that disk had a completely normal, working NTFS/exFAT
  partition. Root cause (`core/drives.rs`'s `get_drive_infos_internal`):
  the code meant to skip a raw physical drive already shown as a lettered
  volume by building a `HashSet` of existing drive-letter path strings
  (`"c:\\"`, `"d:\\"`, ...) and checking whether it `.contains()` the raw
  drive's own path (`"\\.\physicaldrive1"`) - two entirely different path
  formats that can never be equal, so the check silently never excluded
  anything, for every removable/USB-bus physical drive on the system.
  Fixed by comparing the thing both sides actually agree on - the
  physical drive *index* - resolved from each lettered volume via
  `IOCTL_STORAGE_GET_DEVICE_NUMBER` (`physical_drive_number_for_volume`)
  and compared against the index parsed out of the raw drive's own
  `\\.\PhysicalDriveN` path (`physical_drive_index_from_path`). When
  writing a "have I already seen this X" check across two different
  enumeration sources, verify a match actually fires for a real duplicate
  case before trusting it silently prevents one - a dedup set that's
  never actually removing anything is easy to not notice until a user
  reports "many" of something that should only ever show up once.
- **A narrow filesystem-name allowlist (`"NTFS"`/`"FAT32"` only) can wrongly
  flag other perfectly-readable filesystems as unsupported.** The same
  drive-detection code also excluded exFAT (the default on most large USB
  drives/SD cards) and ReFS (Windows' own modern filesystem, used by Dev
  Drive/Storage Spaces) from the "already fine, don't show as unmounted"
  list - meaning a genuinely-unmounted-but-otherwise-normal exFAT/ReFS
  volume got treated the same as a raw Linux/ext partition Windows truly
  can't read. Widened the allowlist to `NTFS`/`FAT32`/`EXFAT`/`REFS`. When
  a filesystem-type check exists to distinguish "genuinely unsupported"
  from "just needs a moment to look right," check it against every
  filesystem this app's normal file APIs can actually read, not just the
  one or two most common ones.

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