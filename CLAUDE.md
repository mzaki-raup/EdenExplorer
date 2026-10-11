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
- **A gesture wired into one view is not wired into "the item viewer" -
  this codebase draws the same folder six different ways, and four of them
  had no background double-click handler at all.** Reported as a *store*
  problem ("why in seeweed, when i double click mouse, it not go to
  parent?"), which sent the first hour into `go_up`, remote path parsing
  and `\\?\EdenRemote\...` - all of which turned out to be correct (a
  three-line probe printing `Path::parent()` for a remote path settled it
  immediately, and is worth writing before reading any navigation code).
  The actual cause was a one-line grep away: `double_click_navigates_up`
  appeared in `itemviewer.rs` and `sqlite_ui.rs` and **nowhere else**, so
  Gallery, Columns, Column+Preview and Preview had never had the gesture,
  for any folder, local or remote - the report only came from S3 because
  that is what the user happened to be browsing. This is the same shape as
  the "handler wired up only inside the `!visible_items_empty` branch"
  entry above, one level out: that one is about a handler covering only one
  *branch* of a view, this one about it covering only one *view* of six.
  **When a user reports a gesture missing in some specific context, grep
  for the setting or action that drives it before investigating that
  context at all** - a hit count far lower than the number of places it
  ought to appear is the answer, and it reframes "a bug in S3" as "a gap in
  four shared views" that fixing once covers every store, present and
  future. Two further things fell out of fixing it: (1) the three
  non-Details views have no `ItemViewerNavBarAction` to write into, so the
  gesture had to become an `ItemViewerAction::NavigateUp` handled centrally
  - which is better anyway, because `Navigation::go_up` knows about This PC,
  drive roots, remote roots, tag and search views, and three views each
  computing their own parent path would each have got one of those wrong;
  (2) the column browser usually does **not** fill its pane, so the dead
  space to the right of a single 180px column belonged to no column's
  background rect - sensing "each column's own width" looked complete and
  left the largest, most obvious target in the view inert. Only live
  testing found that; reading the code it looks finished. If a view's
  content doesn't span its pane, check what owns the leftover space before
  calling a background handler done.
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
- **Bulk Rename's batched `rename_paths_native` (one `IFileOperation`,
  `FOF_ALLOWUNDO` set, one `RenameItem` call queued per item) was observed
  once to report the whole batch as "Failed" while the three source items
  had actually been moved to the Recycle Bin instead of renamed** - a
  genuine, if apparently intermittent, native-call outcome rather than
  anything this app's own Rust code chose to do (recycling isn't a
  fallback path anywhere in the rename handlers). Only observed once
  across many repeated identical attempts (the exact same Find/Replace
  pattern against the exact same three files succeeded cleanly on a
  fresh debug-build relaunch immediately after), so it could not be
  reliably reproduced or root-caused - possibly a transient COM/shell
  hiccup, possibly state left over from an earlier release-build session
  in the same testing pass. No data was lost: `FOF_ALLOWUNDO` means a
  "failed" rename that actually recycled the source is still fully
  recoverable (confirmed via the Recycle Bin's own `OriginalDirectory`
  metadata and restored with the `Item.InvokeVerb('undelete')` COM verb).
  If Bulk Rename ever gets a genuine, reproducible bug report of "items
  disappeared instead of being renamed," check the Recycle Bin for them
  by original directory before assuming data loss - and don't treat a
  single unreproducible "Failed" notification as proof the surrounding
  code (including anything reusing `rename_paths_native`, like Undo/Redo)
  is broken; retry once on a clean relaunch before concluding a
  regression was introduced.
- **A stuck `pinentry.exe` process is the actual root cause behind the
  recurring `gpg: signing failed: Timeout` seen across many sessions in
  this repo** (previously just retried blindly until it happened to
  succeed, sometimes after many attempts). When gpg-agent's passphrase
  cache has expired and a commit needs a fresh prompt, it spawns
  `pinentry.exe` - but if a *previous* prompt was ever left dangling
  (the window closed unexpectedly, the process got orphaned, whatever
  the cause), gpg-agent sees pinentry as already busy and every
  subsequent signing attempt times out waiting for a prompt that will
  never come, no matter how many times the commit is retried. The fix
  is not to keep retrying - check for and kill a stuck pinentry first
  (`tasklist | grep -i pinentry`, then `taskkill //F //IM pinentry.exe`
  from Git Bash), then retry the commit once; gpg-agent spawns a clean
  new prompt and signing succeeds immediately. `gpgconf --kill
  gpg-agent` is a red herring here if run from Git Bash - it's as likely
  to hit a *different* MSYS2 gpg-agent than the one Git for Windows'
  configured `gpg.program` actually talks to, and won't fix a stuck
  pinentry it doesn't own. If a commit fails to sign more than two or
  three times in a row, check for a stuck pinentry process before
  continuing to retry the exact same command.
- **Found the actual root cause behind this session's repeated
  "click/keystroke did nothing" ui-test failures: `focus.ps1` and
  `click.ps1`/`send-keys.ps1` invoked as *separate* Bash tool calls each
  spawn their own fresh PowerShell process - and each new process itself
  briefly becomes the real Win32 foreground window the instant it starts,
  silently stealing focus back from EdenExplorer *after* `focus.ps1`
  already succeeded in an earlier, now-exited call.** Confirmed directly:
  polling `GetForegroundWindow()` mid-test showed the foreground window
  was a `powershell` process (this very ui-test tooling's own process),
  not EdenExplorer, even immediately after a `focus.ps1` call had printed
  "focused EdenExplorer". `click.ps1`'s raw `PostMessage` calls kept
  posting to the right `hwnd` regardless, but the click never took
  effect - and `send-keys.ps1`'s own focus re-check correctly caught and
  aborted rather than sending keys to the wrong window. This looked
  identical to the general "mouse clicks aren't registering" gotcha
  already documented above (and really did happen for genuine
  focus-loss reasons too, e.g. another foreground app) - but the fix
  here is specific and reliable: invoke `focus.ps1` and the
  click/send-keys call **in the same PowerShell process** (e.g.
  `powershell -Command "& '.../focus.ps1'; & '.../click.ps1' -X .. -Y .."`)
  rather than as two separate Bash tool calls, so no new process gets a
  chance to spawn and steal focus in between. Once confirmed working
  once this way, every click/keystroke for the rest of that test attempt
  worked correctly. If a click or `send-keys` stops working mid-session
  despite `focus.ps1` reporting success moments earlier, try this
  same-process chaining before concluding the app itself is unresponsive
  or spending time on other workarounds.
- **The Undo/Redo Replace-safety work (`delete_paths_native`'s internal
  recycle-before-replace call) reused the same `FOF_WANTNUKEWARNING` flag
  the user-facing Delete action uses, and that made an internal
  housekeeping delete pop Windows' own native "Are you sure you want to
  permanently delete this file?" confirmation dialog in the middle of a
  Replace/Copy-undo/Move-redo/Copy-redo operation the user never asked
  to delete anything for.** `FOF_WANTNUKEWARNING` means "warn if this
  item is about to be destroyed rather than recycled" - it's exactly
  right for a real user-initiated delete (matches Explorer's own
  behavior, which is why it was used there originally), but wrong for
  code that's silently recycling an item as an implementation detail
  (making room for Replace, or Undo/Redo's own bookkeeping) - if Windows
  decides for any reason the item can't be silently recycled (locked
  file, recycle bin disabled/full, whatever), that flag surfaces a
  blocking native dialog with no relation to what the user actually did.
  Fixed by adding a `silent: bool` parameter to `delete_paths_native`
  (`mainwindow_imp.rs`): `silent: true` uses `FOF_NOCONFIRMATION |
  FOF_SILENT` instead (no prompt, ever - a failure is just an `Err` the
  caller already checks via `.is_ok()`), used by all of Replace's
  recycle-before-place, Copy-undo's delete-what-it-created, and Move/
  Copy-redo's re-recycle step; the one genuine user-facing call site
  (the `Delete`/`Delete Permanently` context menu entries) kept
  `silent: false` (`FOF_WANTNUKEWARNING`) unchanged. When wrapping a
  native Win32 "are you sure?"-capable API for use as *internal*
  plumbing behind a feature the user didn't directly invoke, check
  whether the exact same flags/call used elsewhere for the *direct*
  user-facing version could ever surface UI on their own - silence
  needs to be an explicit, separate flag combination, not inherited by
  accident from a copy-pasted call site.
- **A modal drawn via `egui::Area`/`egui::Frame::popup` (the paste-conflict
  modal, bulk-rename, and now the new Checksums modal) visually sits on
  top of the item viewer, but does nothing to stop the item viewer's own
  per-frame keyboard handling (`itemviewer_helper.rs`'s `handle_global_
  actions` - Ctrl+A select-all, Delete, Ctrl+C/X/V, etc.) from still
  firing underneath it.** Confirmed by direct, if accidental, live
  reproduction: typing into the Checksums modal's "Compare" text field,
  then pressing Ctrl+A followed by Delete to clear it, correctly cleared
  the text field *and* deleted the real file underneath the modal (sent
  to the Recycle Bin, so not permanently lost, but "typing in a dialog
  deletes your file" is a serious surprise). Root cause:
  `itemviewer.rs`'s `modal_input_blocked` - the flag that's supposed to
  suppress exactly this - only ever checked `tags_state.picker.is_some()`;
  nothing set it for `pending_paste_conflict`/`pending_bulk_rename`/(the
  new) `pending_checksum`, so this exact same risk already existed for
  the first two modals too, just never triggered by a Ctrl+A+Delete
  sequence during their testing. Fixed generally (all three modals at
  once) rather than just for Checksums: `MainWindow::ui` now sets an
  `egui::Context` memory flag (`itemviewer_helper::BLOCKING_MODAL_MEMORY_ID`,
  the same "memory flag" mechanism `global_shortcuts_disabled`'s
  `topbar_hamburger_menu` check already uses) once per frame to whether
  any of the three pending-modal fields is `Some`, and `modal_input_
  blocked` now also reads that flag - no need to thread a new parameter
  through `draw_tab_content`/`draw_item_viewer` and their several call
  sites (Details, Gallery, Preview all read the one computed value).
  When adding a new `egui::Area`-based modal here, don't assume it
  automatically suppresses the view underneath just because it's drawn
  on top and captures its own widgets' clicks - check `modal_input_
  blocked`'s computation includes your new modal's pending-state field,
  or a global keyboard shortcut can reach through it.
- **Clicking a breadcrumb segment (not the empty space after it) navigates
  "up" to that segment instead of entering address-bar edit mode**, and
  the app restores every real tab from the previous session on launch -
  so a `click.ps1` coordinate guessed from an earlier screenshot (before
  confirming the *current* breadcrumb layout) can land on a real
  breadcrumb segment and silently navigate into one of the user's actual
  folders (this happened live: a click meant to open address-bar edit
  mode instead navigated from `D:\ZAKI\APPLIANCES` up to the real
  `D:\ZAKI` folder, which has dozens of real subfolders including
  sensitive ones). No data was touched (confirmed folder contents
  unchanged before/after), but the very next `send-keys.ps1` call sent
  `^aD:\eden-test-tags{ENTER}` into that real file list rather than an
  edit-mode text field, and the app then silently exited (confirmed via
  `tasklist`, not just `focus.ps1`'s report) - root cause not pinned down
  (possibly Enter on a stray selection, possibly unrelated), but the
  fix that actually worked was to click on genuinely *empty* breadcrumb-bar
  space well to the right of the last segment's text (not near/on any
  segment label) to reliably enter edit mode, confirmed by the address
  bar then showing the full path as editable text. If a click meant for
  address-bar edit mode instead changes the current folder, re-screenshot
  before typing further keys - don't chain a blind `send-keys.ps1` after
  an unconfirmed click, especially one with real user folders one level
  up.
- **A tag group named "Mytag" already existed in this app's real,
  persisted `tags.bin` before this session's Multi-tagging feature work
  even started** - it appeared in the sidebar's "Tags" section and in
  the picker's "Add to an existing tag group" list during live testing.
  It is the user's real data, not a fixture; every scratch verification
  for the Multi-tagging feature used newly created groups ("Test Tag A"/
  "Test Tag B") instead, and both were deleted via Settings > Tags'
  trash icon (with an "Are you sure - Tag name: ..." confirmation naming
  the exact group) once verification finished, leaving "Mytag" and its
  2 real items untouched throughout. When a picker/list surfaces
  existing named groups/tags/favorites during a live test, don't assume
  everything shown was created by this session - check whether a name
  is one you actually just typed before clicking it.
- **The user can be actively driving the same live, real EdenExplorer window
  at the same time this skill's scripts are - and their real clicks/keys can
  look exactly like the synthetic-input-not-registering gotcha already
  documented above.** While verifying the Multi-tagging feature, a
  `click.ps1` aimed at a tab suddenly stopped producing any visible effect
  at all (confirmed with a cheap control-click retry, per the existing
  gotcha's own advice), and in between two of my own screenshots the active
  tab jumped to an unrelated real "Search" tab and a real file in
  `D:\DOWNLOADS` appeared mid-rename with an unfamiliar random name - all
  without me sending any click that should have caused it. It turned out
  the user was interacting with the very same window themselves at that
  moment (confirmed by asking). Nothing was actually broken and no data was
  lost, but two real, harmless side effects fell out of it purely from
  their own clicks landing while my scratch tag group happened to be open:
  a real file got renamed (by them) and then tagged (by them, trying out
  the new feature) into my scratch tag group. The fix was just to delete
  the whole scratch tag group at the end as already planned (removing the
  stray membership along with it) rather than trying to surgically reverse
  a rename I wasn't even sure I'd caused. If live-testing produces a change
  you can't attribute to any input you actually sent - not just "clicks
  stopped working" but the active tab/selection/content changing on its
  own - consider that the user might be in the same window before assuming
  a new app bug, and just ask; don't rename/undo real files based on a
  guess about what happened.
- **A `#[serde(default = ...)]` trailing field on a `postcard`-persisted
  struct does not actually get "rescued" field-by-field the way the
  existing gotcha above implies - the whole struct fails to decode, and the
  load path's own fallback (`.unwrap_or_default()`) is what saves it, not
  serde's per-field defaulting.** Verified directly while adding
  `sidebar_width` to `SidebarSectionsSnapshot`: hand-building an "old" byte
  stream missing that one trailing field and feeding it to
  `postcard::take_from_bytes::<SidebarSectionsSnapshot>` returns
  `Err(DeserializeUnexpectedEnd)`, not an `Ok` value with `sidebar_width`
  quietly defaulted and every other field intact. Postcard's format has no
  explicit "end of struct" marker, so its `SeqAccess` can't distinguish
  "no more fields, this struct is done" from "ran out of bytes mid-value" -
  it just tries to read the next field's bytes and errors when there aren't
  enough, before serde's derive-generated per-field `#[serde(default)]`
  fallback ever gets a chance to run. In practice this is still safe here
  *only* because every load function in `core::indexer` already wraps the
  decode in `.ok().filter(|(_, rest)| rest.is_empty()).unwrap_or_default()`
  (or equivalent) - an old save fails to decode as a whole and the caller
  gets a fresh, valid `T::default()` instead of a crash or garbage - but
  that means appending a field to a persisted struct resets *every* field
  in it to default for anyone loading an old save, not just the new one.
  If a future field ever needs true per-field migration (preserving a
  user's other settings while defaulting only the new field), appending
  it to the existing struct isn't enough by itself - it would need its own
  versioned migration path (see how `ThemeSettingsSnapshot`'s
  `THEME_VERSION` check and `LegacyAppSettingsSnapshot`'s `From` impl each
  handle *that* problem instead of relying on `#[serde(default)]` alone).
- **Found the above finding's real-world consequence: `ThemePalette` itself
  hits it, and several "zero visual change until touched" claims made for
  past `ThemePalette` field additions in `CHECKLIST.md` were likely wrong.**
  `load_theme_settings` (`core/indexer.rs`) calls
  `load_or_migrate_bincode_to_postcard::<ThemeSettingsSnapshot>`, whose
  postcard branch is a flat `postcard::from_bytes::<T>(&data)` - no
  `take_from_bytes` + "rest is empty" check, no per-field rescue. Per the
  entry above, this call fails outright (not gracefully) for **any** old
  save missing a field that was appended to `ThemePalette` since it was
  written - and the failure path is `load_theme_settings`'s own
  `_ => { reset_theme_to_defaults(); None }` arm, which resets *every*
  color in *both* palettes, not just the new field. Whether `THEME_VERSION`
  gets bumped alongside the new field makes no difference here - the
  version number is only ever checked *after* a successful decode, so a
  byte-level shape mismatch resets the theme regardless of the version
  check passing or failing. In other words: **every additive `ThemePalette`
  field ever ships as a breaking change for anyone whose `theme.bin` predates
  it**, full stop - there is currently no way to add one without resetting
  existing users' saved colors on their next launch after the update, and
  past CHECKLIST entries claiming otherwise were based on the field-level
  reasoning in the entry above without tracing all the way through to this
  specific load path. If this ever needs fixing for real (rather than just
  accepting "a new Appearance feature resets your custom colors once"),
  the fix has to happen in `load_theme_settings`/`load_or_migrate_bincode_to_postcard`
  itself - e.g. decode into an explicitly-versioned enum of past
  `ThemePalette` shapes and upgrade field-by-field - not by continuing to
  add fields to `ThemePalette` and hoping `#[serde(default = ...)]` covers
  it, which it demonstrably does not for this struct's actual load path.
- **A thin, translucent `Stroke` passed to `Painter::rect_stroke` can render
  with one specific edge nearly or fully invisible, even though the
  underlying `Rect` is provably correct - this is an anti-aliasing/alpha-
  blending rendering artifact, not a layout, sizing, or clipping bug, and
  chasing it as one wastes real time.** The address bar's bottom border
  survived several real, correctly-diagnosed fixes for other bugs (a
  duplicate inner border, a stale row-height budget, an `allocate_ui_
  with_layout` sizing floor that let content grow past its requested size)
  without the missing bottom edge ever actually going away, because none
  of those were the actual cause. It was finally isolated by testing one
  stroke parameter at a time against the exact same `pill_rect`: first a
  temporary `rect_filled` (solid, opaque) over the rect confirmed the
  `Rect` itself was perfectly correct and fully bounded on all sides;
  then swapping the border's color between an opaque test color and the
  real translucent `pill_border` color, with corner radius, width, and
  `StrokeKind` each held fixed in turn, showed the missing edge appeared
  *only* when the color was translucent - an opaque color at the same
  width/radius/kind always drew a complete border. The fix was to
  pre-blend the translucent color against its known background into a
  fully **opaque** `Color32` before calling `rect_stroke`, removing any
  dependence on runtime alpha compositing, rather than adjusting the
  `Rect`, the layout, or the clip rect any further. If a border/stroke is
  missing or broken on only one side and the rect it's drawn against has
  already been confirmed correct (e.g. via a `rect_filled` sanity check),
  suspect the stroke's own alpha before re-investigating layout/clipping/
  sizing again - vary one stroke parameter (color/alpha, width, corner
  radius, `StrokeKind`) at a time against a hardcoded test value to find
  which one is actually responsible, rather than guessing architecturally.
- **Follow-up/correction to the entry above: the opaque-color fix solved the
  outer pill border, but the exact same "one edge silently missing" symptom
  came back almost immediately on the search box's own inner query-field
  border, and the real cause there turned out to be something the previous
  diagnosis never considered - a clip rect cutting off a border drawn flush
  against its own boundary.** `draw_bordered_breadcrumb` hands the address
  bar's content to a child `ui` created via `ui.new_child(...)` with
  `set_clip_rect(content_rect)` set to *exactly* the available interior
  rect (deliberately, so real overflow clips invisibly instead of growing
  the row - see that function's own doc comment). The search box's query
  field draws its own border only 1-2px inset from that same content_rect,
  meaning its stroke sits right at (or past) the clip boundary. Verified
  with the same pixel-level methodology as the entry above (a solid
  `rect_filled` test fill to find the frame's exact bounds, then sampling
  individual pixels at each of its four edges): the color/opacity fix from
  above did **not** help here - even a fully opaque `rect_stroke` at that
  exact rect still rendered with whichever edge(s) touched the clip
  boundary completely absent (not faint - pixel-identical to plain
  background), while the edges with a few more px of slack before the clip
  rendered correctly. Confirmed by then giving `content_ui`'s clip rect a
  few extra px of slack (`content_rect.expand(3.0)`) - not enough to let
  real overflow escape visibly, but enough that a border drawn at the
  content's own edge no longer coincides with the clip boundary - after
  which every edge rendered. **The real lesson: a `Ui`'s `clip_rect` set
  exactly to the space you intend content to fill will also silently eat
  part of a border/stroke that a child draws flush against that same
  boundary - clipping and stroke rendering aren't just "does the rect fit",
  a border drawn *at* the clip edge is itself right on the failure
  boundary.** If a border is missing on the specific edge(s) closest to a
  known clip rect (not just "one edge is always wrong" the way the entry
  above was), suspect the clip rect's own bounds before re-blending stroke
  colors again - give the clip a few px of slack past where content is
  expected to actually reach, rather than sizing it to the exact pixel.
- **A settings-page "edit mode" toggle that only writes to a `selected_*`
  field on the *editor's own* state, with no code path that also updates
  the live value it's supposed to preview, silently does nothing when the
  editor isn't currently showing the active value.** The Appearance page's
  Dark/Light mode buttons (`customizetheme.rs`) only ever set `customizer.
  selected_mode` (which palette the editor displays/edits) - the actual
  rendered theme is a separate `MainWindow::theme` field, only changed by
  the topbar's own theme-toggle icon. `apply_theme_customizer_action`
  (`mainwindow_imp.rs`) only marks the live theme dirty (triggering a
  repaint) `if mode == current_mode` - so clicking "Light mode" while the
  app is running in Dark mode saved every subsequent color edit to disk
  correctly, but produced zero visible change, because the mode being
  edited was never the mode being displayed. This reads exactly like "the
  picker doesn't work" even though the underlying color-application code
  is completely fine. Fixed by making the mode-toggle buttons also flip the
  live theme (a new `ThemeCustomizerAction::SetLiveMode`). When a settings
  page has both "which thing am I editing" state and a separately-tracked
  "what's actually live" state, check that switching the former also
  switches the latter (or intentionally decide it shouldn't, and make that
  obvious in the UI) - don't assume a mode toggle implies a live preview
  just because it looks like one.
- **A color-picker field with no reader anywhere in the codebase looks
  identical to a working one - it renders, it opens a working picker, it
  even sets a real struct field - only nothing draws with that field's
  value.** The Appearance page's "Resize Handle" row edited `palette.
  resize_handle`, a real, fully wired-up field - but the actual sidebar-
  drag-handle paint call (`mainwindow.rs`) read `palette.
  button_seperator_handle_fill` instead (a same-purpose field that existed
  for historical reasons, oddly filed under "Checkboxes & Buttons"). Two
  fields doing the same job, one of them silently dead. `grep`-ing for
  every real field name gets easy to skip once a codebase has ~60+ palette
  fields across a dozen customizer sections; the sort of check that
  actually catches this is per-field, not per-section: for a "this picker
  does nothing" report, grep the *exact* field name (not just the section)
  across the whole codebase and confirm at least one non-customizer call
  site reads it, before assuming the wiring is fine because the section
  "looks complete." Fixed by repointing the real call site to the
  correctly-named field and deleting the dead one outright, rather than
  leaving two fields for one control.
- **A translucent border color blended over a box's own fill color can read
  as fully invisible on every edge, not just clipped/anti-aliased edges -
  and this is a genuinely different bug from the two border-visibility
  gotchas already documented above.** The item viewer's type-to-filter box
  and the preview pane's Find bar both used the established pattern -
  `Frame::NONE.fill(palette.input_field_bg).stroke(Stroke::new(1.0-1.5,
  low_alpha_blend))` - and both were reported as having no visible border at
  all. Pixel-sampling a live screenshot (scanning a full row/column through
  the box, not just one spot) found *zero* border-colored pixels anywhere,
  at any width up to 2.5px and any blend alpha up to ~75% - ruling out both
  previously-diagnosed causes (a missing-edge alpha-blend artifact, a clip
  rect eating a flush-drawn edge) before reaching for a third theory. The
  actual cause: `borders_default` blended toward `input_field_bg` produces
  a color that's still very close to `input_field_bg` itself when the box's
  own *fill* is also `input_field_bg` - low contrast against its own
  background, not literal invisibility, but visually indistinguishable at
  normal viewing distance regardless of stroke width. Confirmed by swapping
  in a diagnostic solid `Color32::RED` stroke first (which rendered
  perfectly), isolating the problem to contrast/color choice rather than
  the paint call itself. Fixed by using a fully **opaque** accent color
  (`palette.borders_active`, already used elsewhere for "this is active/
  focused" state) instead of a translucent blend, plus a genuinely thicker
  stroke (2.0px). If a border is reported invisible and pixel-sampling
  shows literally no border-colored pixels anywhere (not just at one edge),
  suspect low contrast between the blended border color and the box's own
  fill before re-diagnosing as a clip/anti-aliasing issue - and verify a
  color-only fix with a loud diagnostic color (solid red) before spending
  time tuning blend ratios, since that immediately tells you whether the
  stroke is rendering at all.
- **Found the first real case of a `postcard`-persisted struct that
  actually needed a field added to it after the fact, and it confirmed the
  earlier `ThemePalette` finding's practical consequence: appending a field
  directly, with no migration path, would have silently wiped every user's
  existing saved data on their next launch.** `CustomThemeEntry` (a saved
  custom theme's name/accent/secondary) needed a new `palette:
  Option<ThemePalette>` field so selecting a saved theme could restore the
  *whole* palette the user had, not just re-derive it from accent/secondary
  (see the "Custom themes... colors gone" entry in `CHECKLIST.md` for the
  full bug). Per the existing documented finding, `load_custom_themes`'s
  postcard decode (`postcard::from_bytes::<T>`) has no per-field rescue for
  an appended field - it fails the *entire* struct, and the existing
  `Option::None` fallback in `load_or_migrate_bincode_to_postcard` would
  have quietly returned an empty theme list instead of the user's real one.
  Fixed by keeping a private `CustomThemeEntryLegacy`/
  `CustomThemesSnapshotLegacy` pair (the exact pre-change shape) purely as
  a decode fallback: `load_custom_themes` tries the new shape first, and on
  failure decodes the legacy shape and converts each entry to
  `palette: None` rather than losing it outright - re-saved in the new
  shape automatically the next time the user changes anything about their
  themes. **This is the reusable pattern for any future field added to a
  `postcard`-persisted struct that already has real user data riding on
  it** (`ThemePalette` itself still doesn't have this and remains exposed
  per the earlier finding - test coverage: `custom_theme_entry_round_trips_full_palette`/
  `legacy_custom_theme_bytes_without_palette_field_still_decode` in
  `core/indexer.rs`). Don't just add `#[serde(default)]` and assume it's
  covered - that annotation does nothing for postcard's whole-struct decode
  failure mode; write (and keep) a small `*Legacy` shadow struct instead.
- **`Response::request_focus()` alone doesn't stop an *earlier-running*
  global keyboard handler from intercepting the very first keystroke typed
  right after a text field opens.** The preview pane's Find bar
  (`itemviewer_preview.rs`) called `request_focus()` on its query field the
  frame it opened, which looked correct in isolation and was reported fixed
  - but the user immediately reported it "still not working," with typed
  characters landing in the item viewer's own type-to-filter box instead.
  Root cause: `handle_global_actions` (`itemviewer_helper.rs`, home of that
  type-to-filter capture) runs at the very top of `draw_item_viewer`, before
  the Find bar even renders that same frame - it gated its "start filtering
  on any typed character" logic on `ctx.egui_wants_keyboard_input()`
  (`memory().focused().is_some()`), which is a snapshot of whatever *last*
  held focus, not a guarantee that the widget about to render this frame
  will hold it before this check runs. `request_focus()` genuinely does
  update that state - just not early enough in *this* frame's draw order to
  be seen by code that already ran earlier in the same frame. The fix
  wasn't a timing tweak - it was structural: pass an explicit `bool` (`find_
  in_preview_active`) into `handle_global_actions` so it returns immediately
  whenever the Find bar is open, the same way it already does for an active
  rename, instead of inferring "something else has focus" from a
  focus-snapshot race. If a newly-opened/focused widget's very first
  keystroke is getting stolen by an unrelated global key handler elsewhere
  in the same frame, check whether that handler runs *before* the widget in
  draw order and is gating on a focus snapshot rather than an explicit
  "this specific UI is open" flag - `request_focus()` is not a synchronization
  primitive other code run earlier in the frame can see.
- **`egui::ScrollArea::max_height(N)` is a cap, not a guarantee - it computes
  `available_outer.size().at_most(max_size)` (confirmed directly in egui's
  source), so if the surrounding `Ui`'s own idea of "available height" comes
  back smaller than `N`, the ScrollArea silently shrinks to *that*, not to
  `N`.** The notifications bell panel (an `egui::Area`, not a `Window`/
  `CentralPanel`) hit exactly this: `.max_height(PANEL_MAX_HEIGHT)` (480)
  was supposed to let the row list grow to fit up to 3-4 operations
  comfortably, but the panel was reported as "shorter than before" - live
  pixel-sampling showed the *second* row's status icon cut off mid-glyph
  right at the popup's own border, not a scrollbar appearing early: a real
  clip, not a "working as capped" case. Root cause: inside an `Area`
  (which, unlike `Window`, has no persisted size memory to fall back on -
  see the extensive `Window`-size-memory history elsewhere in this file),
  the content `Ui`'s available-height figure came back far smaller than the
  real space below the panel, and `max_height` only ever *caps down* from
  whatever that number is - it can't stretch a too-small available-height
  figure back up to the requested cap. Fixed by explicitly reserving the
  height the actual row count needs (`row_count * an_estimated_row_height`,
  itself capped at the same `PANEL_MAX_HEIGHT`) via `ui.set_min_height(...)`
  *before* creating the `ScrollArea`, so its own `available_outer` can never
  under-report space again. If content inside an `Area`/floating popup gets
  clipped despite an apparently-generous `max_height`, don't assume the cap
  itself is the problem - measure whether the *available* height feeding
  into that cap is the actual culprit (a live pixel-level screenshot showing
  a hard mid-glyph cut, not a scrollbar, is the tell), and reserve a
  known-good minimum height explicitly rather than trusting the ScrollArea
  to compute it from context alone.
- **The app's own hand-painted outer window border (`mainwindow.rs`, a
  `rect_stroke` around `ctx.viewport_rect()` every frame) isn't the only
  border source - `windowsoverrides.rs`'s `apply_window_override` also sets
  the native `DWMWA_BORDER_COLOR` attribute to the same accent color,
  asking *Windows' own compositor* to draw a second, ~1px native border on
  top of it.** Reported as "a double border, thin outer + thick inner, but
  only at the top edge" - the two borders only read as visibly separate
  along the top (DWM's own top-edge border rendering differs from its
  plain left/right/bottom resize-border pixels, which the app's thicker
  paint fully covers). If a *single* border painted by this app's own code
  ever looks doubled, inconsistent between edges, or oddly positioned,
  check `windowsoverrides.rs`'s `DwmSetWindowAttribute` calls
  (`DWMWA_BORDER_COLOR`/`DWMWA_CAPTION_COLOR`/etc., attributes 34-36)
  before assuming the egui-side paint call is the culprit - this app
  currently has *two* independent places asking for a colored border
  around the same window, and only one of them is meant to actually draw
  one. Fixed by setting `DWMWA_BORDER_COLOR` to `DWMWA_COLOR_NONE` (a real
  constant in the `windows` crate's `Dwm` module, `0xFFFFFFFE`) instead of
  an accent color, so DWM stops drawing its own.
- **A layout element positioned with absolute coordinates derived from only
  its own size (not its actual start position) breaks the instant something
  earlier in the same row changes how much space it consumes.** The sidebar
  resize-handle's hit-region and drag-pill (`mainwindow.rs`) were built as
  `egui::Rect::from_min_size(egui::pos2(sidebar_width - separator_width /
  2.0, 0.0), ...)` - correct only because the sidebar column happened to
  start at local x=0 at the time it was written. Adding a `ui.add_space(8.0)`
  before the sidebar column (to fix a separate "no gap on the left" bug -
  see `CHECKLIST.md`) shifted the sidebar's real start to x=8, and this
  handle code kept assuming x=0, so it rendered 8px too far left and
  visually overlapped the sidebar's own now-shifted right edge. The fix
  wasn't to hardcode a new offset - it was to capture the real start
  position (`let sidebar_start_x = ui.cursor().left();`, read right after
  the `add_space`) and use `sidebar_start_x + sidebar_width` instead of
  `sidebar_width` alone. When touching layout code that adds/removes
  leading space in a row (`add_space`, a new sibling, a margin change),
  grep the rest of that function for any absolute `pos2`/`Rect` math that
  assumes a column starts at a fixed coordinate - it's an easy thing to
  miss because the code that breaks isn't textually near the line you
  actually changed.
- **An `egui::Painter::hline`/`vline` drawn at exactly a container's own
  `available_rect_before_wrap()` bound renders flush against whatever sits
  at that same edge - including the app's own outer window border, if nothing
  insets it.** The split-pane active-indicator line (`ACTIVE_PANE_INDICATOR_
  THICKNESS`, `mainwindow.rs`) used `accent_rect.x_range()` directly for both
  the primary and secondary pane, so the secondary (right) pane's indicator
  touched the outer border with zero gap - every other bordered element near
  it (the address-bar pill, panel corners) keeps a small margin from its own
  container edge, but a raw `hline`/`vline` doesn't do that automatically the
  way a `Frame`'s own margin does. First fix attempt inset the range by only
  2px on each end - looked correct in a live-zoomed screenshot of a plain
  folder-view pane, was reported *fixed*, then reported *still broken* by
  the user against a Settings tab in the same split pane. Exact pixel
  sampling (not just a visual look at a crop) found why: the app's outer
  border isn't a hairline at the exact viewport edge, it's a real ~3-4px
  painted band (`Stroke::new(3.0, ...)` centered on `viewport_rect().
  shrink(1.5)`), and a 2px inset lands the indicator's edge *inside* that
  band's own pixel span rather than before it - the first pane just
  happened to have a hair more natural clearance, making 2px look
  sufficient there by coincidence. Bumped to 6px, which reliably clears the
  border's own span with margin regardless of what's drawn inside the pane.
  **Two lessons, not one: (1) an inset needs to clear the *painted width* of
  whatever it's avoiding, not just be "a small positive number" - measure
  the actual border's pixel span before picking a value; (2) verifying one
  instance of a bug (one pane, one content type, one crop) and calling it
  fixed doesn't rule out a *narrower* margin at some other instance of the
  same code path - re-check with exact pixel sampling (`Bitmap.GetPixel` at
  specific coordinates, comparing against known color values), not a visual
  glance at a screenshot crop, especially after a user reports "still
  happening" following a claimed fix.** This bug could only be caught by
  actually toggling split view live and sampling pixels at the corner - a
  single-pane screenshot never renders this line at all, so code review
  alone would have missed it entirely both times.
- **A `#[serde(default = ...)]` attribute on a new `ThemePalette` field does
  NOT protect it, no matter how reasonable that looks next to the dozens of
  other fields already using that same attribute - `ThemePalette`'s actual
  load path decodes the whole struct in one shot with no per-field rescue
  (this was already documented above as a *theoretical* finding; this entry
  is what it looks like when it actually happens).** Added a `tab_gap: f32`
  field to `ThemePalette` (with `#[serde(default = "default_tab_gap")]`,
  matching the pattern used everywhere else in the struct) for a new Tab Gap
  setting - and on the very next launch, every color in both the user's
  dark and light palettes silently reset to the app's stock defaults
  (confirmed by pixel-sampling the "new" accent color against `base_color()`'s
  literal `(110, 85, 160)` - an exact match). The user's actual customized
  accent color was gone for good; it had never been backed up and the reset
  defaults were already written back to `theme.bin` before this was
  noticed. A second, independent crash followed from the corrupted file
  left behind (`cursor has Nan` inside `sidebar.rs::draw_section_header`,
  confirmed via a debug-build panic backtrace) - unrelated to the palette
  fields themselves, but every subsequent launch crashed until `theme.bin`
  was deleted to let the app regenerate a clean one. **The fix was not a
  data-recovery one - it was architectural: `tab_gap` was pulled out of
  `ThemePalette` entirely** and given its own dedicated small file via
  `core::indexer` (`TabLayoutSnapshot`/`load_tab_layout`/`save_tab_layout`,
  using the *safe* `take_from_bytes` + "rest is empty" decode pattern
  `SidebarSectionsSnapshot` already uses - not the unsafe whole-buffer
  `from_bytes` pattern `ThemePalette` and `AppSettingsSnapshot` both still
  use), exactly mirroring how `sidebar_width` already avoids this same trap
  (see `ThemeCustomizer::sidebar_width`'s own doc comment). **Before adding
  any new field to `ThemePalette` (or `AppSettingsSnapshot` - it has the
  identical unsafe decode path), ask whether it actually needs to vary by
  dark/light mode or be bundled with the rest of that struct at all - if
  not, give it its own tiny file the way `sidebar_width`/`tab_gap` do,
  rather than trusting `#[serde(default = ...)]` to make an append safe.**
  It doesn't, for either of those two structs, regardless of how many other
  fields already carry that attribute as if it were protection.
- **A `ui.painter().rect_stroke(rect, ...)` drawn at exactly a `StripBuilder`
  cell's `ui.max_rect()` renders flush against whatever sits at that cell's
  edge - including the app's own outer window border, for whichever pane is
  last/only.** The item viewer's "subtle box border around the file/folder
  view" (`explorer.rs`, `content_rect = ui.max_rect()` inside a `StripBuilder`
  cell) has no margin of its own on any side; its top/left/bottom are always
  internal (the navbar strip above, the sidebar or split divider to the
  left, the status bar strip below), but its *right* edge is the pane's own
  right bound, which for the last/only pane coincides with the outer
  border's own ~3-4px painted span. Both strokes landed in the same pixel
  band, reading as one slightly-thicker line - confirmed only by exact pixel
  sampling (`Bitmap.GetPixel` at specific coordinates against known color
  values), since both colors were purple-tinted and visually blended at a
  glance. Fixed by pulling in only the rect's right edge before stroking
  (`rect.max.x -= 6.0`), leaving the other three sides untouched since they
  never touch the outer border. When a border/stroke is drawn against a
  `StripBuilder` cell's or `ui.max_rect()`'s raw bounds, check which of its
  edges can ever coincide with the app's own outer border (usually just one,
  not all four) before reaching for a uniform `.shrink()` - insetting every
  side loses margin that was already fine on the internal edges for no
  reason.
- **Re-querying `ui.available_height()` *inside* a plain `ui.horizontal(|ui|
  {...})` closure does not return the parent's remaining page height - a
  horizontal layout's row has no inherent height bound of its own (it grows
  to fit whatever's tallest inside), so the value inside can be far larger/
  effectively unbounded compared to what the same call returns just before
  entering the closure.** Splitting the Appearance settings page into two
  side-by-side columns (`customizetheme.rs`), each sized via
  `ui.allocate_ui_with_layout(egui::vec2(column_width, ui.available_height()),
  top_down, ...)` called *from inside* a `ui.horizontal(|ui| {...})` wrapping
  both columns, silently broke both columns' sizing - `ui.available_height()`
  queried at that point returned something disconnected from the page's real
  ~640px remaining height, and every inner `ScrollArea` (even with
  `auto_shrink` disabled) collapsed to a ~100px sliver with hundreds of px of
  visibly dead space below it. A quick `eprintln!` of `ui.available_height()`
  right before entering the `horizontal` (not inside it) showed the correct,
  sane value, pointing straight at the closure boundary as the actual cause.
  Fixed by capturing the height into a local *before* entering
  `ui.horizontal`, then reusing that captured value for both columns' sizing
  instead of re-querying it from inside. When a layout looks like it's
  fighting for space it should obviously have (a page reports a sane
  available-height figure at the top level but content still collapses),
  suspect a `ui.available_height()`/`ui.available_width()` call made *inside*
  a differently-laid-out child closure (`horizontal` vs. the parent's own
  layout direction) before assuming the sizing math itself is wrong - print
  the value at both points and compare rather than guessing.
- **Nesting one auto-shrinking/scrollable layout directly inside another
  general-purpose one silently breaks both, and the fix is to opt the inner
  one out of the outer entirely, not to tune either one's parameters.** The
  Appearance settings category previously rendered inside the Settings
  page's own shared `settings_content_scroll` `ScrollArea` (used by every
  category, with `auto_shrink([false, true])`) and its `SETTINGS_CONTENT_
  MAX_WIDTH` (720px) width cap - both entirely reasonable for a single
  scrolling reading column, both wrong once Appearance needed its own
  independent two-column scrolling layout (settings list left, static
  preview right - see the entry above and `CHECKLIST.md`). Trying to make
  the *inner* layout coexist with the outer one (adjusting `auto_shrink`,
  explicit `.max_height()` calls) never actually fixed the collapsed-height
  symptom, because the outer ScrollArea's own auto-shrink-to-content
  behavior was measuring against a child that itself wanted to fill
  "whatever's available" - a circular sizing dependency neither side can
  resolve alone. The actual fix was structural: special-case
  `SettingsCategory::Appearance` in `settings.rs` to skip both the shared
  `ScrollArea` and the width cap entirely, rendering directly into the full
  available space with its own self-contained scrolling instead. If a
  newly-added nested scroll/auto-sizing layout inside an existing one
  produces a squashed-with-dead-space-below symptom that survives several
  parameter tweaks, check whether the *outer* container needs to stop
  wrapping that specific content at all, rather than continuing to adjust
  either layout's own settings against each other.
- **Passing a master-detail page's two columns as `FnOnce` closure
  *arguments* to one shared layout-splitting function does not borrow-check
  when both columns need a mutable borrow of the same state, even though
  the function only ever runs them one after the other internally.** The
  first cut of a shared `master_detail_columns(ui, left_closure,
  right_closure)` helper (written for the Favorites/Custom Context Menu/Tab
  Groups/Tags redesign) failed to compile everywhere it was used, because
  both closures are constructed - and their captures borrow-checked - as
  soon as they're passed as sibling arguments to the same call, regardless
  of the fact that the function body only calls one, then the other. Two
  closures that each need `&mut settings`/`&mut tags_state`/`&mut favorites`
  fail here even though there's no real aliasing at runtime. This is
  different from the sequential-reborrow pattern that *does* work (see the
  Appearance page's own two-column split): that one runs
  `ui.allocate_ui_with_layout(...)` calls as two separate **statements**
  inside a single closure, not as two closures passed to an outer function.
  Fixed by changing the shared helper to just return `(column_width,
  column_height)` and having each page write the same `ui.horizontal(|ui|
  { allocate_ui_with_layout(left); allocate_ui_with_layout(right); })` shape
  inline itself. If a helper meant to reduce duplication across pages needs
  to run two pieces of caller-supplied UI code that both touch the same
  mutable state, don't pass them in as two `FnOnce` parameters to one call -
  return whatever's actually shared (sizes, ids, rects) and let each call
  site write the sequential statements itself.
- **A row's own click-to-select sense registered *after* drawing the row's
  content (including its own buttons) silently steals every click meant for
  those buttons, not just clicks on the row's empty space.** The master-
  detail redesign's shared `list_row` helper first drew the row via
  `Frame::show(...)`, then added a separate `ui.interact(frame_response.
  response.rect, ...)` for row selection - since egui gives click priority
  to whichever overlapping sense was registered *later*, and this row-select
  sense was registered after the buttons drawn moments earlier inside the
  frame, it won every click in the row's rect, including ones landing
  exactly on a reorder/delete button. Confirmed live: clicking a Favorite's
  down-arrow changed the *selection* instead of moving the row. This is the
  exact same rule already documented above for the tab strip's own drag
  background, just easy to get backwards when starting from a `Frame::show`
  return value (which only gives you the rect *after* content already drew)
  instead of computing the rect first. Fixed by reversing the order:
  `ui.allocate_exact_size(..., Sense::click())` for the background rect and
  sense *first*, then paint the fill/border into that rect via
  `ui.painter()` directly (not `Frame::show`, which would draw-then-return),
  then draw the row's actual content (the buttons) afterward via
  `ui.scope_builder` positioned at that same rect - the buttons' senses now
  register after the background's, so they correctly win for their own
  pixels. When adding a row-click-to-select pattern on top of a row that
  also contains its own interactive buttons, sense the background rect
  *before* any button in the row is drawn, never after.
- **`setting_row`'s own `ui.add_space(ui.available_width())` spacer (used to
  push its right-aligned controls over) claims the *entire* remaining row
  width before that right-aligned closure ever runs - so any code inside
  that closure that itself calls `ui.available_width()` gets back whatever's
  left *after* the spacer already took it all, not the width it visually
  has room for.** This was already flagged as a "known follow-up" (the
  General settings page's Language dropdown going invisible in a narrow
  split pane) but got hit for real during the Favorites/Custom Context Menu/
  Tab Groups/Tags master-detail redesign: every plain `TextEdit` field sized
  itself via `[ui.available_width().min(260.0), ...]`, intending the `.min`
  as a safety cap - instead it silently collapsed every one of those fields
  down to a barely-visible sliver, because the "available" figure it was
  clamping against was already near-zero by the time it was read. Fixed by
  using a flat, fixed size (`[260.0, ...]`) instead, matching how these
  fields worked correctly before the redesign. If a widget inside
  `setting_row`'s right closure needs to know how much room it actually has,
  don't call `ui.available_width()` from in there - it doesn't mean what it
  looks like it means in this specific helper.
- **`fill::TAG` and `regular::TAG` share the same Unicode codepoint, so
  passing the "fill" glyph constant alone renders the outline icon anyway -
  the actual filled weight only draws if the text is *also* given the
  `egui::FontFamily::Name("phosphor_fill".into())` family override**, since
  Phosphor's Fill weight is registered as its own separate named family, not
  merged into `Proportional`. This was already documented once (the
  sidebar's own tag icons, `sidebar.rs`) but got made a second time writing
  the new Settings > Tags master-detail page's group-list icon - it used
  `regular::TAG` with no family override at all, rendering as a thin
  outline instead of matching the sidebar's solid-filled look the rest of
  the app uses for the same icon. Whenever pairing an `egui_phosphor::fill::
  *` glyph with a color (a strong visual signal like a tag/status color is
  usually the point of reaching for the fill weight over the outline one),
  double-check the `RichText`/label call also sets `.family(FontFamily::
  Name("phosphor_fill".into()))` - the glyph constant alone is not enough,
  and nothing will warn you if you forget it; it just quietly renders the
  wrong weight.

- **Two pages that call the exact same shared UI helper function can still render visibly different spacing, if that helper's spacing depends on `ui.spacing()`/`ui.style()` state that an earlier, *page-specific* widget call left the `ui` in - and whether that state actually leaks through depends on a detail as easy to miss as "was this button called directly on the outer `ui`, or inside its own `ui.horizontal(|ui| ...)` closure."** The Favorites/Custom Context Menu/Tab Groups/Tags master-detail pages all share one `list_row(ui, ...)` helper (duplicated per file by this codebase's own convention), including an identical trailing `ui.add_space(6.0)` meant to gap each row from the next - yet Custom Context Menu's parent-menu rows rendered with the rows touching directly (confirmed via pixel-sampling: zero background-colored pixels between them), while Tab Groups' showed a small but real gap, from what looked like byte-identical code. Root cause: `eden_button` calls `apply_eden_visual_overrides(ui, palette)` internally, which shrinks `item_spacing`/`interact_size` on whichever `Ui` it's actually given - Tab Groups' single "Add Group" button is called directly on the page's own outer, function-level `ui`, so that shrink persists forward into the list's `ScrollArea` (a child `Ui` created later inherits the *current*, already-mutated style); Custom Context Menu's "Add Command"/"Add Submenu"/"Export"/"Import" buttons are each wrapped in their own `ui.horizontal(|ui| {...})`, so the override only ever touches that closure's local child `Ui`, which is dropped once the closure returns - the outer `ui` the list later renders with never sees it. Two pages, same helper, same explicit `add_space` value, genuinely different total visible gap (automatic `item_spacing.y` plus the explicit spacer), because one page's earlier button call leaked a style mutation forward and the other's didn't. Fixed by having `list_row` pin `ui.spacing_mut().item_spacing.y = 0.0` at its own top, so the visible gap is controlled *only* by its own explicit trailing `add_space` (bumped to 8.0), never by whatever ambient state a page's unrelated earlier buttons happened to leave behind. If a shared helper's spacing/sizing ever looks inconsistent across two call sites using "the same" code, check whether an earlier widget call at each site mutated the passed-in `Ui`'s style directly vs. only a nested closure's own child `Ui` before assuming the helper itself is bugged - and verify with pixel-sampling (`Bitmap.GetPixel` scanning a column through the boundary, checking for the real page-background color) rather than a screenshot glance, since a 0px-vs-2px difference is easy to miss by eye but shows up immediately in raw pixel values.

- **A scrollbar-thumb "drag" simulated with `drag.ps1` at a guessed x-coordinate can land on a nearby interactive control instead of the actual scrollbar, and dragging across an already-open color picker's own saturation/hue square silently sets it to whatever arbitrary color the drag path crosses.** While live-testing the new Settings → Appearance "Badge Color" picker, a drag meant to move the color-picker column's scrollbar thumb (guessed at a fixed x just past the visible swatches) instead started on the Badge Color swatch itself once scrolled to that row - opening its popup and dragging through it changed the color to an arbitrary value with no relation to the intended default. Caught immediately by pixel-sampling the swatch afterward and finding it didn't match the field's known default RGB (a screenshot glance alone would likely have missed a plausible-looking wrong color). Fixed by clicking "Reset Theme" - safe in this specific case only because the rest of the palette was already sitting at stock defaults from an unrelated, separate cause (see the entry below), so nothing else was lost by a full reset. If a genuinely customized palette is ever in this situation, "Reset Theme" is NOT the safe fix - it discards every other field too; re-set just the corrupted field's color explicitly instead. When simulating a scrollbar drag by coordinate guess (no dedicated scroll script exists in this project's ui-test skill), verify the guessed x-coordinate is actually past every color swatch's own column, not just past the labels - and pixel-sample any color-picker field the drag path crossed afterward, the same way a click's landing spot already gets verified elsewhere in this file.
- **Adding a field to `ThemePalette` reset a real, already-customized `theme.bin` to stock defaults during this session's own live verification** - not a hypothetical, the exact consequence the existing `ThemePalette`/postcard finding elsewhere in this file already predicted, observed actually happening. Confirmed via pixel-sampling an unrelated field (`Status: Success`) against its literal stock-default RGB and finding an exact match post-relaunch, right after a `badge_color: Color32` field was appended for the new circular-badge feature. This is the accepted, pre-existing limitation already documented at length elsewhere in this file (search for "every additive `ThemePalette` field ever ships as a breaking change") - not a new bug, and not something this task fixed - but it's worth a second, shorter entry here specifically as a reminder that it isn't just a theoretical risk: **any session that adds a `ThemePalette` field should expect it to happen for real, mention it to the user afterward (their custom colors are gone, not just "possibly at risk"), and point them at Export Theme (if they'd already used it) or manual re-customization** - don't assume "it compiled and the picker works" means nothing else was affected.

- **A plain `ui.label(...)` is selectable text by default under this app's style, and that silently steals clicks meant for a row it's drawn inside of.** The Favorites/Custom Context Menu/Tab Groups/Tags master-detail pages' left-column rows are built on a shared `list_row` helper whose whole design already depends on egui's later-registered-sense-wins rule (background sensed first via `allocate_exact_size`, then content/buttons drawn and sensed after, so buttons correctly win their own pixels - see the entry above on this same rule). What wasn't accounted for: a bare `ui.label(icon)` / `ui.label(title)` inside that content isn't purely decorative the way it looks - by default it's *also* a click-and-drag-sensing selectable-text widget, registered after the background just like the buttons are, so per the exact same rule it silently won every click landing on the icon glyph or title text, while clicks on genuinely empty row background (between the text and the buttons) worked fine. Confirmed live and reproducibly: clicking squarely on a row's label/icon left the previously-selected row selected; clicking a few px to the right on bare background correctly switched selection. This codebase had already hit this once before in a narrower form (Custom Context Menu/Favorites' icon-path row uses `egui::Label::new(&path_text).truncate().selectable(false)`, added for a different, unrelated overflow reason) without generalizing the lesson to every other label in a clickable row. Fixed by converting every icon/title `ui.label(...)` call inside these four files' row-content closures to `ui.add(egui::Label::new(...).selectable(false))`. **Whenever a `ui.label`/`RichText` sits inside anything meant to be clickable as a whole (a custom row, a card, a selectable list item) - not just inside `list_row` specifically - assume it's implicitly grabbing clicks meant for the container unless it's explicitly `.selectable(false)`, and verify by literally clicking on the text/icon pixels during live testing, not just on the row's visible-but-otherwise-empty background**, since a test that only ever clicks background space (an easy habit once a row "looks" uniformly clickable) will never catch this.

- **A hardcoded fallback-width constant that's always smaller than a fixed "minimum width" constant can silently start returning a width *below* that minimum, the instant the minimum becomes user-configurable and can exceed it.** `tabs.rs`'s `compute_tab_layout` had a "few tabs, plenty of room" branch that capped the computed tab width at a flat `PREFERRED_TAB_WIDTH` (180px) - completely safe as long as `MIN_TAB_WIDTH` was a fixed constant (110px, always less than 180). Making the minimum a user-configurable Appearance setting (0-300px range) broke that unstated invariant: setting it above 180 could produce a computed tab width *narrower* than the user's own configured minimum whenever there were few enough tabs for the "natural" per-tab width to legitimately exceed it, because the flat 180px cap still applied regardless. Caught by code review (tracing through the branch's own logic against the new wider input range) rather than live-testing, since it only manifests for one specific setting value combined with one specific tab count. Fixed by capping at `PREFERRED_TAB_WIDTH.max(min_tab_width)` instead of the flat constant. When a constant that was implicitly always-smaller-than-another-constant gets replaced by a user-configurable value, check every place that other constant is used as a cap/floor against it - a relationship that held for free with two fixed numbers doesn't hold automatically once one of them can move.
- **This app's Settings window has no keyboard-only path to switch between its own category sidebar entries (General/Behavior/Appearance/...) - only a mouse click does it, and arrow keys/Tab do nothing there.** Confirmed directly while trying to verify the new Minimum Tab Width setting during a session where mouse clicks (`click.ps1`) were confirmed unresponsive (the already-documented recurring gotcha elsewhere in this file): `Ctrl+Tab` successfully switched the *outer* file-manager tabs to reach the Settings tab, but once there, `{DOWN}`/`{TAB}` produced no visible change in which settings category was selected - unlike the tab strip itself, or Bulk Rename's modal (which does support Tab+Space navigation), the Settings category list apparently isn't part of egui's keyboard focus traversal at all. If mouse clicks are confirmed broken during a session that needs to verify something inside a specific Settings category other than whichever one is already open, there is currently no keyboard workaround - don't spend time trying arrow keys/Tab on this specific list, and don't assume "keyboard navigation exists for the outer tabs" extends to it.

- **A dedicated icon font can still define real, non-empty glyphs outside its own reserved codepoint range - "an icon font never overlaps normal text" is an assumption, not a guarantee, and this codebase paid for trusting it once already.** Fixing a report of Phosphor icons rendering as CJK on systems with a replaced "Segoe UI" (that replacement font happened to squat Phosphor's private-use codepoints), the first fix inserted `egui_phosphor::Variant::Regular`'s own font bytes at index 0 of the Proportional/Monospace family - ahead of the user's chosen font - on the reasoning that a dedicated icon font could only ever define private-use glyphs, so giving it unconditional priority could never break normal text. Shipped, then live-tested: ordinary lowercase text broke app-wide ("This PC" → "T PC", "My User Home" → "M U H", every lowercase letter gone) the moment this ran. Root-caused by inspecting the actual bundled `Phosphor.ttf` with `fonttools` (`TTFont(...).getBestCmap()` + `f['glyf'][name].numberOfContours`) rather than continuing to guess: it genuinely maps real, 1-contour glyphs onto plain lowercase ASCII codepoints (unclear why - possibly leftover from whatever build tool generated it - but verified as fact, not theory). Fixed by actually subsetting the font (`pyftsubset --unicodes=E000-F8FF`) into a new `assets/PhosphorIconsOnly.ttf` bundled asset, rather than reusing the crate's own font data directly - the subsetted copy structurally cannot contain a stray Latin glyph, verified via the same cmap inspection (0 non-PUA codepoints). **Before giving any font blanket priority over another (inserting it ahead in a family's fallback list), verify its actual codepoint coverage with a real tool (`fonttools`' `getBestCmap`/`glyf` table, or equivalent) instead of reasoning from what the font is "for" - and if you can't verify it's exclusive to the range you need, subset it down to that range explicitly rather than trusting the assumption.** `pip install fonttools` worked directly in this environment (`python`/`pip` present, no network issues) when this was needed - reach for it rather than assuming subsetting isn't available.
- **Multiple `focus.ps1`/`click.ps1`/`send-keys.ps1` calls in a row, each right-clicking to reopen a popup menu at whatever position the mouse currently is, silently drift the menu's on-screen position with every retry - a coordinate that worked for one right-click is not safe to reuse for the next one.** While live-testing the new empty-folder background context menu, a sequence of "right-click, screenshot, click item at the coordinates from that screenshot" attempts kept missing - because each subsequent click (even ones that appeared to fail) could itself register as a real click somewhere (reopening the menu at a new position, dismissing it, or actually landing on a different menu item than intended), so a coordinate captured from screenshot N was frequently already stale by the time the click for it posted. This produced two real, if harmless, side effects: an extra scratch item ("New File.txt") got created in the test folder from a stray click that was assumed to have missed, and a leftover "search box" UI state appeared and had to be dismissed - neither was a bug in the app, both were artifacts of chained clicks each being trusted based on a screenshot that was no longer current. The fix that actually worked: right-click, screenshot immediately, click the target from that exact screenshot with **no other clicks in between**, and if a click's effect isn't visible in the very next screenshot, re-right-click fresh rather than trying to "correct" the previous attempt with another guessed coordinate. For anything that reopens a popup at the cursor position (as opposed to a fixed on-screen location), prefer a stable, always-in-the-same-place trigger (a toolbar button) over a right-click menu when the test only cares about the resulting behavior, not the menu itself.
- **This app has a search-mode toggle on the address/breadcrumb bar, separate from plain address-edit mode, and clicking too far right within a *narrower* pane (e.g. one half of a split view) can hit it instead of empty breadcrumb space.** A click aimed at empty breadcrumb space (the documented technique for entering address-edit mode) landed on the search toggle instead once panes got narrower in a split-view layout, opening a "Search files and folders..." box with its own scope-toggle icons instead of the plain editable path text. Typing a path into it and pressing Enter/Escape produced confusing intermediate states (the folder navigated correctly, but the address bar stayed showing raw editable text, and Escape reverted the tab to a previously-open real folder rather than just closing the search). If a click meant for address-edit mode instead opens something with a magnifying-glass icon and extra scope buttons, that's the search toggle, not the address field - click further left (closer to the last real breadcrumb segment) or re-verify the click target's exact bounds in a fresh screenshot before assuming address-edit mode opened, especially in a split-pane layout where each pane's breadcrumb is significantly narrower than the single-pane case any earlier screenshot's coordinates came from.
- **`egui_extras::TableBuilder` draws a resize handle after every column marked `.resizable(true)`, including the last one - even though there's nothing to its right to actually resize against.** The Details table's Tags column (the last visible column in the default order) was built with `.resizable(true)` like every other column, and rendered a stray vertical separator line trailing after its own content, with empty dead space between that line and the pane's real right edge/border - visible in both panes of a dual-pane split, reported by the user as "why is there an additional right border line after the tags column." Since this app's columns are user-reorderable (`column_layout.ordered_columns`), "the last column" isn't a fixed name that can be special-cased directly - fixed by tracking the loop index against `ordered_columns.len() - 1` while building the `TableBuilder`'s columns and calling `.resizable(false)` only on whichever column happens to end up last that frame. If a table's trailing column ever shows an unexplained extra vertical line past its own content, check whether it's still marked resizable - a column with nothing after it doesn't need a resize handle, and egui_extras draws one anyway unless told not to.
- **egui 0.35's built-in `Button` widget computes its frame's `inner_margin` from each interaction state's own *default* `bg_stroke.width` (read from `Widgets::dark()`/`light()`, `inactive` = 0.0, `hovered` = 1.0) - and it reads that default *before* a caller's own `.stroke(...)` builder override is applied, so overriding the stroke color doesn't stop the margin from shifting.** Every themed button in this app (`eden_button` and the dialog primary/secondary/ghost helpers, `core/utils/widgets.rs`) passed a constant `.fill(...)`/`.stroke(...)` pair regardless of hover state, on the reasonable assumption that meant the button would look and size identically at rest and on hover - instead every one of them visibly grew/shrank by ~1-2px on hover, reported by the user as buttons resizing app-wide with no accompanying color change to explain it. Traced by reading `Button::atom_ui`'s actual source (not guessing): `button_padding = frame.inner_margin` is captured from `ui.style().button_style(&classes, state)`'s state-dependent frame *before* this app's `.fill()`/`.stroke()` overrides are applied to that same `frame` variable - the override changes what gets *painted*, not the margin arithmetic that already ran using the ambient per-state default. Fixed by not using `egui::Button` for these at all: a new `draw_themed_button` helper allocates a fixed-size rect once (from the text's own measured size), then only ever repaints which precomputed fill/text color goes into that same rect depending on hover - the same fixed-rect-then-repaint-color technique `clickable_icon_sized_with_base_color` (same file) already used successfully, which is what suggested the fix once the `Button`-based approach was understood to be unfixable through its own builder API. If a themed `egui::Button` with an explicit `.fill()`/`.stroke()` override still visibly resizes on hover despite the passed-in colors being state-independent, don't keep tuning the override - check whether `ButtonStyle`'s own per-state `bg_stroke.width` (or `expansion`) differs before the override is applied, and consider building the button by hand instead of fighting `Button`'s internal state machine.
- **In an immediate-mode GUI, clearing the flag that gates a modal's draw call does NOT make that modal disappear on the current frame - only on the next one, and if the click handler itself blocks before returning, "the next frame" doesn't happen until it does.** The paste-conflict modal's Replace/Rename buttons visibly froze on screen for a noticeable delay after being clicked, reported by the user. `handle_paste_conflict_resolution` (the click handler) took `self.pending_paste_conflict` - clearing it - as its very first statement, which looked like it should make the modal vanish immediately... except this frame's modal draw calls were already queued *before* the click handler even ran (the modal drew itself earlier in the same frame's update pass, using the state as it was *before* the click was processed). The state change only affects what the *next* `update()` call draws - and since this handler then went on to do real synchronous work (a full Recycle Bin enumeration for Replace, a folder-size calculation for any pasted folder, both taking real wall-clock time), the current frame - modal included - didn't finish and reach the screen until all of that completed. The fix wasn't "clear the flag sooner" (it was already first) - it was moving the actual slow work off the click handler entirely, onto a background thread, so the handler returns near-instantly and the very next frame has nothing left to draw. If a modal/dialog in this codebase (or any immediate-mode UI) visibly "hangs" after a button click despite the corresponding `Option`/bool being cleared as the first line of that click's handler, look at what runs *after* that line in the same function - anything doing real I/O or computation there blocks the frame the modal needed to disappear on, not a future one.
- **A stray click in a split-view pane, aimed at "empty breadcrumb space" to enter address-edit mode, instead landed on a real item and opened its "Add Tag" dialog - and the very next click, meant to close that dialog, instead landed on a completely different real folder shown in the *other* pane and applied a real tag ("Rust") to it.** This is the worst-case realization of the exact risk the two entries above (and the Safety Rules section) already warn about: a coordinate guessed from a stale screenshot, chained immediately into a second blind click, produced a genuine unwanted mutation of the user's real data (`D:\GITHUB\RUST\EdenExplorer` gained a tag it never had) rather than just a wrong selection or a missed click. Caught immediately from the next screenshot (a green "Rust" tag badge visible where none had been), not from any error or crash - a mutating click can succeed exactly as posted and look completely unremarkable in a screenshot unless the *before* state is specifically remembered and compared. Remediated by right-clicking the affected folder and using its own "Remove Tag" context-menu entry (confirmed by a follow-up screenshot showing the Tags column empty again) - not by any bulk/undo action, since none existed for this. **The lesson beyond "verify the click target before firing": when a chained click sequence produces an unexpected dialog or unfamiliar UI state (here, "Add Tag" appearing when an address-bar click was intended), stop and re-screenshot before sending the *next* queued click "to close it" - that recovery click is exactly as likely to miss and land somewhere real as the click that caused the problem, and chaining a blind dismissal onto an already-wrong state is how one stray click became two.** If this ever happens again: stop immediately, screenshot to confirm exactly what changed and where, and use the app's own reverse action (Remove Tag, Undo, Delete, etc.) rather than guessing at a recovery click.
- **The recurring mouse-click-unresponsive state (already documented twice above) happened again while live-verifying the custom-theme Primary/Secondary editing feature, and this time it left the user's real, live Dark theme's Secondary accent color changed to an unintended value (blue) with no way to revert it within the session.** While testing, a batch of chained clicks (open Primary picker, click hue slider, click elsewhere to close) landed unpredictably - one hue-slider click that should have only affected Primary instead left Secondary changed to blue, most likely because an earlier click in the same batch missed its intended target and landed on the Secondary swatch instead, reopening a stale picker for the wrong field before the intended click even fired (the same "chained clicks drift" root cause already documented above, just this time changing a real value instead of missing or selecting the wrong row). Attempting to revert it hit a wall: precise hex-field text entry (click field, `Ctrl+A`, type, Enter) silently failed to update the value on every attempt despite the field visibly showing a text cursor after the click - not a targeting problem, since a **control click test right after** (clicking the unrelated "General" settings category) also produced zero effect, confirming mouse input was fully unresponsive, not just missing this one widget. A fresh `Stop-Process -Force` + relaunch (which has fixed this before) did not help this time - post-relaunch, `Ctrl+Tab` (keyboard) correctly cycled through file-manager tabs to reach Settings, but once there, no keyboard path exists to switch **Settings' own category sidebar** (General/Behavior/.../Appearance) - already a documented limitation - so there was no way to keyboard-navigate back to the Appearance page's color pickers at all. **This is the actual, durable lesson beyond "confirm with one cheap control click before concluding clicks are broken" (already documented): when a color/value editor is being tested and a stray click might have changed something, verify with a control click *immediately*, before firing more of the batch - by the time a whole planned sequence has run and a screenshot is checked, an unresponsive-click window may have already let an earlier click in that same sequence silently do nothing while a later one (assumed to target something else) actually landed correctly on the wrong widget.** No file data was touched or lost (this only affects one theme color field, trivially correctable by the user themselves in two clicks once mouse input is normal for them), but it's the first time this recurring input issue has left the *app's own real settings* in a changed state that a session couldn't revert, rather than just a missed test action - worth extra caution before batching multiple clicks against any live, real (non-scratch) settings value, not just real files.
- **A derived color can be mathematically correct - genuinely computed from the right source field, on every edit, with no bug in the formula - and still read as "not following" to a user, if the blend weight toward that source is too weak next to a full-strength reference of the same source shown nearby.** Reported twice as "Toolbar Icon Color still not follow secondary": traced every read/write of the field and found it already correctly tinted from `secondary_accent` (`tint(WHITE-or-dark-neutral, secondary_accent, 0.55)` in `regenerate_base_derived_colors`) - no bug, nothing to fix, per the first investigation. The user asked again anyway, and this time live-sampling the actual running app's pixels (not just re-reading the code) found the real, non-bug explanation: with Secondary at `(255,214,64)`, `pinned_tab_color` (also secondary-derived, but used at full strength with no white blend) sampled at an exact `(255,214,64)` match, while `toolbar_icon_color` sampled at `(240,218,143)` - correctly tinted, but only lightly, so it read as a pale cream next to `pinned_tab_color`'s full gold swatch right above it in the same settings page. **The lesson: when a user insists a derived value "isn't following" after code review already proved the wiring is correct, don't re-conclude "not a bug" a second time from code alone - pixel-sample the actual live values and compare the weak derivation against a strong one from the same source field.** A formula that's "technically correct" but visually washed out is a real, actionable complaint, not a misunderstanding - the fix here was raising the blend weight (0.55 → 0.85), not further explaining why 0.55 was "correct."
- **While investigating the entry above, the live theme's Primary/Secondary colors visibly changed between several of this session's own read-only screenshots (blue→red→teal, with matching Custom Themes/Live Preview swatches each time) despite this session sending no click to any color picker in between - direct evidence the user was actively driving the same live window concurrently**, the same scenario an earlier entry already documents. A scrollbar-drag aimed at the Appearance page's settings-list scrollbar (using a previously-reliable fixed x-coordinate) landed on the Secondary swatch instead and opened its color picker, because the content had shifted position under that coordinate from the user's own concurrent edits - the same "stale coordinate from a previous screenshot" root cause already documented elsewhere in this file, just triggered by someone else's clicks moving the target instead of this session's own. Recovered safely by pressing `{ESC}` (not clicking anywhere, which risks landing on the hue/saturation square and changing the value - see the entry above) and confirming the hex value (`B77C21`) was identical before and after. If a live theme's colors are shifting on their own between screenshots with no corresponding action from this session, stop assuming stable coordinates and either ask the user directly or fall back to a purely code/unit-test-based investigation instead of continuing to click.
- **A scrollbar-drag whose *start* coordinate lands on a swatch instead of the scrollbar can still fire that swatch's `.clicked()`, even though the gesture moves 100+ px away before releasing - `Sense::click()` alone (no drag sense) doesn't require staying near the press point, only that the release still falls within the same widget's rect.** While live-verifying the new "persist the last-selected custom theme across a restart" feature, a `drag.ps1` scroll aimed at the Appearance page's settings-list scrollbar (`-Y1 300 -Y2 440`) landed its *press* on the "AsusNuc13Pro" custom theme swatch (the page hadn't scrolled from where an earlier screenshot assumed it had) - and despite moving 140px away before releasing, this session's brand-new persistence code faithfully did its job: `editing_palette` was replaced with that theme's full saved palette, `new_custom_theme_name` was set to its name, and (working exactly as designed) `selected_custom_theme.bin` was written to disk. Confirmed only after the fact, via a genuinely fresh `Stop-Process` + rebuild + relaunch cycle done as part of testing: the restarted app immediately showed "AsusNuc13Pro" pre-filled with an "Update Theme" button, which is what proved both that the feature works *and* that the live theme had been changed without an intended click. Not data loss (a saved custom theme the user themselves created, not corruption), and the user had already been cycling through many presets themselves throughout the same session, so the practical severity was low - but it's a new lesson distinct from the already-documented "a drag can land on a swatch and open its picker" entry: **that entry was about the drag's *own* endpoint opening a picker; this one is about the drag's *start* point alone being enough to fire a plain click-sense widget, regardless of how far the drag then travels.** When scrolling via a coordinate-guessed drag near any click-sense grid (swatches, list rows), verify the *press* coordinate specifically lands on empty scrollbar track in the most recent screenshot - not just "this x-coordinate has worked before" - since page content shifting under a stale assumption turns the drag's start point into an ordinary click on whatever's actually there.
- **Follow-up to the incident above: the first "Remove Tag" fix didn't actually stick - a `Stop-Process -Force` (`build-and-launch.ps1`'s own old-instance-cleanup step, used routinely between edit/verify cycles) can discard an in-memory mutation that was never given a chance to reach disk, and the very next session relaunch showed the "Rust" tag back on the real `EdenExplorer` folder as if the removal had never happened.** This was confirmed, not assumed: re-doing "Remove Tag" a second time and checking `%LOCALAPPDATA%\ExplorerEden\tags.bin`'s own `LastWriteTime`/`Length` immediately after showed the file genuinely does get rewritten synchronously on each tag mutation (189 bytes → 161 bytes, timestamp updated within the same second) - so tag saves are NOT batched/debounced in a way that would explain the loss by itself. The actual, verified-safe fix was closing the app via its own window close button (confirmed the process actually exited via `Get-Process -Name EdenExplorer` returning nothing) rather than `Stop-Process -Force`, then relaunching and confirming the Tags column stayed empty - which it did. **Whichever mechanism is true (a debounce/batching window this test didn't happen to catch, some other in-memory-only state, or something else entirely), the practical rule is the same: after performing a corrective/undo action on real user data during a testing session (not scratch data), close the app gracefully and re-verify the fix survived a real relaunch before considering the incident resolved - don't assume a screenshot showing the corrected state is durable, and don't force-kill the process as the very next step after making a correction you need to stick.** `build-and-launch.ps1`'s automatic `Stop-Process -Force` cleanup is fine for ordinary edit/rebuild/relaunch cycles where nothing pending needs to be saved - the risk is specific to the narrow window right after a manual data-correction click.

- **A native Win32 file dialog (`rfd::FileDialog::save_file()`/`pick_file()`) breaks `focus.ps1`/`screenshot.ps1`'s own `Get-Process | Where-Object { $_.MainWindowTitle -eq "EdenExplorer" }` lookup while it's open, and `{ESC}` sent via `System.Windows.Forms.SendKeys` did not reliably cancel it either.** Clicking "Export Theme" opened a real Save dialog; the very next `focus.ps1` call printed `NOT FOUND: EdenExplorer is not running` even though `tasklist` confirmed the process was still alive - the dialog's own window (a different title, not "EdenExplorer") becomes the process's reported main window while it's modal, so the existing title-based lookup can't find it. Sent `{ESC}` via a standalone `SendKeys` call (not `send-keys.ps1`, since that script's own focus pre-check would have aborted for the same reason) intending to cancel - but the very next screenshot showed the dialog gone, EdenExplorer's own file list back, *and* a real export file (`eden_theme_dark.json`) freshly written to the user's actual `D:\DOWNLOADS` with today's timestamp, meaning the dialog actually **saved**, not cancelled. Root cause not pinned down (possibly the dialog had already been positioned with Save as its default button and something about the keystroke or its timing triggered that instead of Cancel) - what matters operationally: **don't assume `{ESC}` cancels a still-open native save dialog just because the app's own window later looks normal again; verify by checking whether the target file actually exists/changed, not just by screenshotting the main window.** No data was lost here (the write was to a plain export JSON file, not any of the user's own files, and it happened to be the export this session was already trying to test) - but a genuinely-intended cancel could easily have looked "successful" from a screenshot alone while a real file was written anyway. If a task needs a native file dialog opened and then safely dismissed without saving/picking anything, treat it as effectively untestable via this skill's current scripts rather than trusting an `{ESC}` keystroke blind.
- **A modal native file dialog left open mid-test can silently accept a stray Enter/click from something else entirely and turn a planned "just open Export Theme to check it renders" into a real, uncontrolled file write.** Directly following from the entry above: because the dialog wasn't confirmed dismissed via any positive signal (only inferred from "the next screenshot looks normal"), the actual mechanism that closed it is unknown - it could just as easily have been a leftover queued keystroke, focus returning and some default control activating, or genuine cancel-then-something-else. Whenever a task is going to trigger a native OS dialog (file picker, save/open, a Windows "Are you sure" prompt) as a side effect of a UI click, treat that as a stopping point requiring explicit confirmation of the dialog's actual final state (does the expected file exist with the expected content, or does it not exist at all) before concluding anything about what the dialog "did" - a plausible-looking following screenshot of the main window is not sufficient proof either way.
- **Every `rfd::FileDialog::new()` call in this app opened its native dialog *ownerless* (no parent HWND), and that's not just a `focus.ps1` inconvenience - it's a real visual bug users can see.** Reported directly with a screenshot: opening Export Settings made the main window's own top-right corner render a bare native Close box and a stray context-help "?" glyph, replacing this app's own themed title bar/controls for as long as the dialog stayed open. Root cause: this app strips `WS_CAPTION` and paints 100% of its own window chrome by hand every frame (see `apply_window_override`) - there's no real native non-client area for Windows to fall back on. A dialog opened via `rfd::FileDialog::new().save_file()`/`.pick_file()`/`.pick_folder()` with no `.set_parent(...)` call blocks the main window's message loop without Windows ever being told "this window is this dialog's owned/disabled parent" - so if the OS needs to redraw the frozen main window for any reason while the dialog is up (focus change, a monitor/DPI event, the classic "unresponsive window" ghosting Windows does for a blocked message queue), it falls back to a minimal *native* frame instead of trusting the app to keep painting its own (which it can't, since it's blocked). Fixed generally across all 13 call sites in the app (Export/Import Settings, several folder pickers, custom icon/command pickers), not just the one reported: added `windowsoverrides::dialog()` as a drop-in replacement for `rfd::FileDialog::new()` that calls `.set_parent(&Win32DialogOwner(hwnd))` - a small wrapper implementing `raw_window_handle`'s `HasWindowHandle`/`HasDisplayHandle` (both are required by `set_parent`'s bound, not just the former) around the app's real HWND - and a `MAIN_HWND` global stash (mirroring the existing `EGUI_CTX`/`set_egui_ctx` pattern in the same file) so the many call sites living in `*_ui.rs` files, which only ever see an `&mut egui::Ui` and never the real HWND, can still reach it without a new parameter threaded through every settings page. **If a future native dialog call is added anywhere in this app, use `windowsoverrides::dialog()` instead of `rfd::FileDialog::new()` directly** - a raw `rfd::FileDialog::new()` call is exactly this same bug waiting to reproduce.
- **A synthetic cross-pane drag-and-drop (`WM_LBUTTONDOWN` at the source row, several `WM_MOUSEMOVE`s with the button held, `WM_LBUTTONUP` over the destination pane, all via `PostMessage`) could not be made to register as a real drag in this app, across multiple retries with progressively finer-grained movement (5 steps at 40ms up to 25 steps at 30ms with extra settle time before release).** Every attempt just re-selected the source row (visible as a highlighted row after the "drag") with no drop occurring - the destination pane stayed empty and its own focus/content never changed. This is a distinct failure mode from the already-documented "real double-clicks and modifier-held clicks can't be reliably synthesized with `PostMessage`" gotcha (`click.ps1`'s own doc comment) - that one is about click timing, this is about `PostMessage`-injected mouse-move-with-button-held events not being recognized by egui's own drag-threshold/hover-target tracking as a continuous drag session, at least not across the two different item-viewer widgets a cross-pane drop requires crossing. If a future change needs live verification specifically of drag-and-drop *between two panes* (as opposed to reordering within one list, which `drag.ps1` already handles fine for text-selection-style drags), treat that as unverifiable via this skill's current scripts and fall back to careful code-path tracing instead of spending more time on synthetic drag variations - this was tried at reasonable length before giving up.

- **Writing a protocol client from memory of its API is a bad bet, and a
  twenty-line probe script settles it - every one of three stores
  contradicted an assumption that would have shipped.** Before writing
  `meilisearch`, `qdrant` and `solr`, each server was started in Docker and a
  script printed the real shape of every endpoint the viewer would use,
  including the failure cases. All three disagreed with what the code would
  otherwise have assumed: Meilisearch reports a `total` on a document page
  (so the walk can stop on knowing rather than probing for an empty page) and
  a per-index `updatedAt` (a real Modified date, which would simply have been
  left blank); Qdrant's `next_page_offset` **is not a number** - it comes back
  as whatever type the collection's ids are, so parsing it as an integer
  works against every integer-keyed collection and fails against every
  UUID-keyed one, a bug that only ever appears on somebody else's data; and
  Qdrant answers a missing API key in **plain text**, not JSON, so any error
  path assuming a JSON body would have turned the one error users actually
  hit into something cryptic. Solr's cursor rule is the sharpest of the three:
  a walk is finished when the returned `nextCursorMark` **equals the one just
  sent**, and the final page arrives *full* rather than empty - so "page until
  a batch comes back empty", which is correct for every other store here,
  loops for ever on a core whose size is an exact multiple of the page. **Cost
  of the probes: a few minutes each. Cost of not doing it: three defects, two
  of them invisible against your own test data.** Start a server and print
  what it actually says before writing the client, and include the failure
  cases in the probe - the error body's shape is part of the API.
- **A `try`/`catch` around a step that a verification loop already exists to
  recover from is not optional once `$ErrorActionPreference = "Stop"` is set.**
  v1.11.1 published with only its zip attached. The release workflow has had a
  verify-the-assets-and-re-upload loop ever since v1.5.0 went out with no
  program (see that entry) - but the *first* upload loop above it called
  `Invoke-RestMethod` bare, the 69 MB `EdenExplorer.exe` came back HTTP 524 (a
  proxy timeout, and quite possibly having uploaded anyway), and under `Stop`
  that threw and ended the script before the recovery loop ever ran. The
  pre-flight guard passed too, because it checks what is **staged** on disk,
  not what landed on the release. **The lesson generalises past this
  workflow: adding a recovery path does nothing if the failure it recovers
  from is fatal before it is reached - check what happens to the error, not
  just that a retry exists.** Both loops now catch and report, letting the
  verification decide what actually landed. The fix was parsed with
  `[System.Management.Automation.Language.Parser]::ParseInput` before
  committing, per the technique recorded below; a published tag is never
  revisited, so v1.11.1 has to be fixed or deleted by hand.
- **Follow-up to the entry above, and the more useful half of it: a 524 on an
  asset upload is *routine* on this forge, not exceptional - and a
  verification loop only protects what it actually checks.** The very next
  release (v1.12.0) exercised the new try/catch and proved it: **both** the
  35 MB zip and the 69 MB exe came back `524`, so without the catch the job
  would have died on the first one and published a bare tag. The retry loop
  recovered the exe on its second attempt - the upload had evidently
  completed server-side despite the proxy giving up on it, which is what a
  524 usually means. **But the zip stayed missing, because the loop only
  compared against `$required` (the exe and `SHA256SUMS.txt`, the updater's
  own contract) - so the catch turned a loud failure into a silent omission
  for every asset outside that pair.** That is a regression introduced by the
  fix, not by the original bug, and it is the generalisable part: *adding a
  catch moves the decision to whatever runs afterwards, so the thing that
  runs afterwards has to cover everything the catch now swallows.* Fixed by
  retrying every **staged** file rather than just the required pair, keeping
  the hard failure for the required pair alone (the zip is a convenience; a
  release without it still installs) and emitting a `::warning::` for
  anything else still missing. The retry pause also went 5s → 15s, since a
  524 means the upload was slow rather than rejected and asking again
  immediately just races the same request. v1.12.0's own zip has to be
  attached by hand - a published tag is never revisited.
- **`Set-Content` writes CRLF on Windows, and that silently made every
  published `SHA256SUMS.txt` useless to the tool it exists for.** GNU
  `sha256sum -c` reads the trailing carriage return as part of the file name,
  so it reports `'EdenExplorer.exe'$'\r': No such file or directory` for every
  line and verifies **nothing** - while still exiting 0, which is how this
  went unnoticed across a dozen releases. The script's own docstring promised
  the file was "usable with `sha256sum -c`", so this was a real defect rather
  than a cosmetic one; it only surfaced because the file was actually run
  through `sha256sum -c` for the first time while repairing v1.12.0's assets
  by hand. Fixed by writing the file with `[System.IO.File]::WriteAllText`
  and an explicit `` `n `` join instead of `Set-Content` - **there is no
  line-ending switch on `Set-Content`**, so a pipeline into it always gets the
  platform's own terminator. **The thing worth generalising: a format whose
  only consumer is an external tool has to be fed to that tool at least once,
  because nothing else can tell you it is wrong** - the app's own reader was
  perfectly happy (`str::lines()` strips `\r` and the name is trimmed), which
  is exactly why the defect survived. That tolerance is now a pinned
  requirement rather than an accident
  (`both_lf_and_crlf_checksum_files_are_read`), because every release
  published before this still carries CRLF and must keep verifying for ever.
- **`cargo build`'s "never used" means "the binary never uses it", not "nothing
  uses it" - deleting on that basis breaks the tests.** `bson.rs`'s `document`
  and `array_raw` were the only warning left in the tree and were reported as
  dead code for days; the obvious move, and the one this file's own
  no-speculative-abstraction rule seems to call for, is to delete them. They
  are used four times - all inside `#[cfg(test)]` modules, by the stand-in
  MongoDB server that turns arbitrary JSON into BSON and therefore nests
  documents that no command this client actually sends does. The tell is
  cheap and worth making a habit: **`cargo test --no-run` emits no such
  warning**, because that build genuinely does use them, so comparing the two
  builds' warnings separates "unused everywhere" from "unused by the binary".
  A plain `grep` for the method name also finds the call sites immediately -
  the only reason this nearly went wrong is that the warning is phrased as an
  absolute. Fixed with `#[cfg(test)]` rather than deletion, which states the
  real situation and keeps the methods for the first command that needs a
  nested document.
- **Deleting a context menu deletes every feature whose only entry point it
  was - check each entry for a keyboard equivalent before removing one.**
  Replacing the terminal's right-click menu with direct copy/paste was the
  whole request, and Copy and Paste were exactly what the right button would
  now do. But that menu also carried **Select All** and **Clear**, and a grep
  of the key handler showed neither had any shortcut: Ctrl+C, Ctrl+Shift+C,
  Ctrl+Shift+V and Shift+PageUp/Down were the entire set. Removing the menu
  as asked would have quietly removed two features rather than relocating
  them - the kind of loss that is invisible in review because the diff only
  shows a menu going away. They became Ctrl+Shift+A and Ctrl+Shift+K. The
  cheap habit: for each entry in a menu being removed, grep the keyboard
  handler for its action, and treat anything with no hit as a feature that
  needs somewhere else to live before the menu goes.
- **CI/CD here runs on a self-hosted Forgejo instance, not GitHub, and that rules
  out most of what a "standard" GitHub Actions pipeline would reach for.** A
  release pipeline was designed around `semantic-release` + `actions/checkout` +
  `actions/setup-node` before checking how the sibling project
  (`D:/WORKSPACES/RUST/flex-taskbar`) actually does it on the same instance -
  all of which had to be thrown away. The constraints, all of them documented in
  that project's own workflow comments: (1) **no published actions at all** - the
  runner fetches them from `code.forgejo.org`, which this instance cannot
  authenticate against (forgejo#2809), *and* they would need Node on the runner;
  the checkout is hand-rolled (`git init` + `git fetch` + a
  `http.extraheader=Authorization: Basic …` credential passed **on the command**,
  never written into `.git/config`). (2) **one runner, one shared working copy** -
  every job starts by emptying it, so two workflows on the same commit delete each
  other's files mid-build; that is why `ci.yml` deliberately excludes `main`, where
  the release workflow runs. (3) `npm`/npmjs reachability is not a safe assumption
  if `code.forgejo.org` isn't reachable - anything needing an `npm install` at CI
  time is a liability, which is why version/notes logic lives in plain PowerShell
  (`scripts/next-release.ps1`) instead. **Before writing or changing any CI here,
  read `flex-taskbar`'s `.github/workflows/*.yml` first** - it is the known-good
  reference for this infrastructure, and re-deriving these constraints costs a
  whole design pass.
- **If `semantic-release` is ever reconsidered for this repo, four of its
  behaviours are counterintuitive enough to have each produced a silently broken
  pipeline during the attempt above** (all four verified empirically against the
  real plugin, not guessed): (1) `releaseRules`' `release: false` is **not a
  veto** - commit-analyzer takes the *highest* release type across **every**
  matching rule, and rule order is irrelevant, so one catch-all
  `{"message": "*", "release": "patch"}` silently overrides every exclusion and
  makes a README typo (and the `chore(release)` commit itself) ship a release.
  (2) Exclusions therefore have to live *inside* the catch-all's own pattern as a
  negated extglob (`!(Document*|chore*)`). (3) Patterns are matched with
  **micromatch**, so `(`/`)` are extglob syntax - `chore(release):*` never matches
  anything, escaped or not - and `*` refuses to cross `/` while `**` refuses to
  cross a newline, meaning a subject like `Add Tab Groups/Send To export` matches
  no rule at all and cuts no release. (4) `parserOpts.headerPattern` **must be a
  real RegExp object**; a string (the only thing a `.releaserc.json` can hold) is
  silently ignored and every commit then parses with no `type`. Combined with the
  angular preset discarding unknown types - which empties the release notes
  completely for this repo's plain-English subjects - the "working" config needed
  a `release.config.mjs` with a RegExp, custom `writerOpts.transform`, and a
  negated-extglob catch-all. The PowerShell implementation that replaced it needs
  none of that, which is the other reason it won.
- **The published release assets are a contract with the app's own update check,
  not a naming convention.** `EdenExplorer.exe` must be published under exactly
  that name (the updater downloads it in place - a versioned filename breaks it),
  `SHA256SUMS.txt` must be published beside it in `sha256sum` layout (`<hex><two
  spaces><name>`) or the updater has nothing to verify against and refuses to
  install, and the tag must be plain `vMAJOR.MINOR.PATCH` with nothing after the
  three numbers (no `-rc1`). The version in `Cargo.toml` is bumped **before** the
  binary is built, and the tag points at that bumped commit, because the updater
  compares a release's tag against the running copy's own `CARGO_PKG_VERSION` - a
  tag that disagrees with the `Cargo.toml` it was built from would offer the same
  update for ever.

- **`ureq`'s TLS defaults to `RootCerts::WebPki` (Mozilla's own bundled roots),
  and combined with this app's `TlsProvider::NativeTls` that fails every HTTPS
  request outright** with `native-tls: unable to find any user-specified roots in
  the final cert chain` - native-tls treats the WebPki bundle as *user-specified*
  roots rather than falling back to the OS store, so nothing validates. Found
  live, not by review: the new update check's own "Check for Updates" button
  reported exactly that string in its modal, against a host (`git.zacx.net`)
  whose certificate `curl` on the same machine accepts without complaint. The fix
  is one builder call - `.root_certs(ureq::tls::RootCerts::PlatformVerifier)` -
  which is what makes native-tls use the roots it loads by default, i.e. the
  Windows certificate store. **`core::remote::webdav::agent()` had exactly the same bug, and
  `core::remote::s3` shares that one function (`use super::webdav::agent`) - so
  every HTTPS WebDAV *and* S3 remote was broken, not just suspected.** Confirmed
  by reproducing it rather than reasoning from the shared code shape: a test
  against the unfixed agent failed with the identical "unable to find any
  user-specified roots" string, and passes with the call added. That test is
  kept as `the_shared_agent_can_complete_a_real_tls_handshake` in `webdav.rs`,
  `#[ignore]`d because it needs the network - run it with
  `cargo test -- --ignored tls_handshake` after touching any agent config. There
  are only ever two agent constructors in this codebase (`core::update::agent`
  and `core::remote::webdav::agent`); both now carry the call, so copy either
  when adding a third. If an HTTPS feature ever fails with a cert-chain error
  against a host that plainly has a valid certificate, check this before
  suspecting the server.
- **A modal drawn with `egui::Area` renders in `MainWindow::ui`'s own call order,
  so a modal opened *from* another modal has to be drawn after it or it is
  painted over and looks like it never opened.** The update modal was first added
  alongside the other modals (`draw_paste_conflict_modal`/`draw_bulk_rename_modal`/
  `draw_checksum_modal`, all around line 1900 of `mainwindow.rs`), but the thing
  that opens it is About's own "Check for Updates" button - and
  `handle_draw_about_window` runs *later* in the same function, drawing a
  full-screen scrim plus its own window straight over the update modal. The
  symptom was indistinguishable from "the button did nothing": the click was
  genuinely handled, the background thread genuinely ran. What proved the click
  had worked was a non-visual side effect - `%LOCALAPPDATA%\ExplorerEden\
  update_state.bin` had just been rewritten with a fresh `last_check` timestamp -
  which is worth remembering as a technique in itself: **when a UI action appears
  to do nothing, look for a side effect it should have left on disk before
  concluding the input never landed**, since this app's own ui-test tooling has a
  long documented history of clicks genuinely not registering and the two look
  identical on screen. Fixed by moving `draw_update_modal` to immediately after
  `handle_draw_about_window`.

- **The very first real release (v1.0.0) went out with empty release notes and
  the wrong title, even though the workflow sent both and the job went green.**
  The publish step POSTed `name` = "EdenExplorer 1.0.0" and `body` = the
  generated notes; what actually landed on the release was `name` = "v1.0.0"
  (the annotated tag's own message) and `body` = "". The cause is that pushing
  an annotated tag already makes the forge list a release for that tag, so by
  the time the publish step runs the release row exists and the fields the POST
  carries are not the ones that stick. Nothing failed - the assets attached
  correctly to that same release, so the job reported success and the defect was
  only visible by actually reading the published release back. **Two lessons:
  (1) after any write through the forge's API, read the object back and assert
  on the fields you care about rather than trusting the write's own return
  value; (2) "the workflow went green" is not the same as "the release is
  right" - check a first release's actual rendered content, not just its exit
  status.** The first fix - look the release up by tag, `PATCH` when it
  exists, `POST` when it does not, then read back once and throw on an empty
  body - **was not enough, and the way it failed is the real lesson.** v1.0.1
  came out perfect and v1.0.2 came out with the tag's own name and no body, from
  byte-identical workflow code, with the job green both times and the read-back
  guard passing both times. Every release row's `created_at` equals its tag's
  commit time, including the ones that came out right, which says the forge
  derives the row from the pushed tag **asynchronously** - so a `PATCH` can land
  *before* that derivation and simply be overwritten by it seconds later, long
  after a one-shot read-back has already confirmed it. **A single read-after-write
  only proves the value was right at that instant; when something else is also
  writing, it proves nothing about the value that survives.** The real fix
  applies the metadata *last* (after the assets) and then confirms it in a
  retry loop with a pause between attempts, re-`PATCH`ing until the title and a
  non-empty body are still there when re-read, and failing the job if they never
  stick. v1.0.0's and v1.0.2's own notes have to be fixed by hand in the forge's
  UI - the workflow will not revisit an already-published tag.

- **A startup cleanup that deletes the previous version's files races with the
  process it is cleaning up after.** `core::update::clean_leftovers` originally
  made one attempt at deleting `EdenExplorer.old.exe` on the first frame. But
  `restart()` spawns the new copy and *then* exits, so for a moment both are
  alive and Windows still holds the old image open - the delete fails, the
  `let _ =` swallows it, and ~65MB sits there until some later start happens to
  win the race. Observed for real after the v1.0.1 self-update. The staged
  download in `%TEMP%\EdenExplorer Update` had the opposite problem: nothing
  removed it *at all*, so every update leaked another program-sized file. Fixed
  by retrying the old-image delete on a helper thread (so the first frame never
  waits on it) and by sweeping the staging directory both at startup and right
  after a successful `install`. **When writing "clean up what the last run left
  behind" code, check whether the thing being cleaned up could still be alive -
  a single attempt at process start is not enough if your own restart path is
  what created the file.**

- **A custom context menu command cannot be waited on, so refreshing the view
  after it is a watching problem, not a timing one.** The obvious fix for "my
  script's output does not appear until I refresh" is to wait for the launched
  process and reload when it exits - and for the real-world command this was
  built for that would have been useless. That command is a `.vbs` that calls
  `shell.Run ..., 0, False` (explicitly *not* waiting), which starts a detached
  `cmd`, which runs a `.bat` that appends to a queue and exits; only whichever
  instance wins a lock folder then runs the PowerShell script that does the
  actual work, seconds later and for however long a zip takes. The process this
  app launched is gone almost immediately, and nothing it can see is still
  running - so `WaitForSingleObject` on it would return just as uselessly early
  as the old flat 900ms timer did. `core::dir_watch` watches the folder instead
  (`FindFirstChangeNotificationW`), which works regardless of how many processes
  deep the real work is. **Two things to keep in mind if that module is ever
  touched: (1) a file being written signals continuously, so refreshes are rate
  limited to one per `MIN_REFRESH_GAP` with a guaranteed final one when the
  folder goes quiet - without that, zipping a large folder re-lists its parent
  hundreds of times mid-operation; (2) the watcher only exists while a command
  might still be working (it stops after `QUIET` with no change), so normal
  browsing costs nothing, and the per-frame check in `MainWindow::ui` is a
  relaxed atomic load rather than a lock.**

- **The single-instance CLI forwarding was written but had never worked, and
  reading the code made it look fine.** `EdenExplorer.exe <folder>` is supposed
  to hand the path to an already-running instance, which opens it in a new tab.
  Two independent bugs stopped it, and both were only visible by running it:
  (1) `main.rs` created the single-instance mutex **only when a path was given**
  (`if launch_options.new_window || launch_paths.is_empty() { None }`), so an
  instance started the ordinary way - no arguments, from the Start menu or a
  shortcut, which is virtually always - never registered itself, and a later
  CLI invocation found nothing to forward to; (2) `forward_paths` located the
  window with `FindWindowW(None, "EdenExplorer")`, but the viewport is built
  `.with_title_shown(false)` and the app strips `WS_CAPTION` to paint its own
  chrome, so the window text was **empty** and that lookup matched nothing -
  forwarding then failed with `WindowNotReady` after retrying for three
  seconds. Fixed by registering the instance regardless of arguments, setting
  the window text explicitly in `apply_window_override` (invisible, since there
  is no caption, but it is what `FindWindowW` and the ui-test scripts both
  match on), and adding an `EnumWindows` fallback that finds a visible
  top-level window owned by another `EdenExplorer.exe` process - so the lookup
  no longer depends on a title at all. **The lesson is not about Win32: this
  was confidently reported as working on a code read alone, and a single
  command would have disproved it. Anything whose whole purpose is to talk to
  another process has to be exercised against a real running instance before
  it is described as working.**

- **`GetSystemMetrics(SM_CXSCREEN)` is the *primary* monitor's width, not the
  desktop's - and `main.rs` used it to decide whether a saved window position
  was still valid, so the window could never be restored onto a second
  monitor.** The guard read `x < screen_w`, which is exactly right for one
  screen and silently wrong for two: any saved position far enough to the right
  failed it and the window was re-centred on the primary instead. Verified on a
  real two-monitor setup - a saved position comfortably inside the second
  screen was discarded and the window jumped back. Fixed by checking the saved
  rectangle against the **virtual desktop** (`SM_XVIRTUALSCREEN`/
  `SM_YVIRTUALSCREEN`/`SM_CXVIRTUALSCREEN`/`SM_CYVIRTUALSCREEN`, whose origin
  can be negative for a monitor placed left of or above the primary) and by
  bounding the restored size by the virtual desktop rather than the primary -
  `with_clamp_size_to_monitor_size(true)` still trims it to whatever monitor it
  lands on. The centring *fallback* deliberately still uses the primary, since
  that is what a fresh install should do.
- **Two things that cost real time while testing the above, both about
  measurement rather than the app.** (1) **A DPI-unaware process is lied to
  about the desktop.** A first pass reported both monitors at 96 DPI / 100%;
  the second monitor is actually 144 DPI / 150%, and the whole virtual desktop
  is 3840 wide, not 3200. PowerShell is DPI-unaware by default, so every
  coordinate and size it reads back is virtualized - call
  `SetProcessDpiAwareness(2)` before `GetWindowRect`/`EnumDisplayMonitors`/a
  screen capture or the numbers are quietly wrong. (2) **This app's window
  positions are stored as logical points, not physical pixels** (they come from
  `i.viewport().inner_rect`), so hand-writing a physical coordinate into
  `window_position.bin` to simulate "it was on the second monitor" tests
  something that never happens - on a 150% screen it is restored 1.5x further
  out. The honest test is to move the real window, let the app's own per-frame
  save run, then close and relaunch: done that way the round-trip is exact
  (closed at physical (2200,150) on the 150% monitor, reopened at (2200,150)).
- **`Process.MainWindowHandle` points at the wrong window while this app is
  minimized**, and that is the source of the long-recorded `focus.ps1` false
  negative. Normally it resolves correctly (the real 1885x775 window titled
  `EdenExplorer`), but this process also owns a small untitled helper window,
  and while the main window is minimized `MainWindowHandle` resolves to *that*
  instead - so `MainWindowTitle` reads empty, the old
  `Where-Object { $_.MainWindowTitle -eq "EdenExplorer" }` filter matched
  nothing, and the script reported "EdenExplorer is not running" for an app
  that was running fine. Focusing the helper also did nothing visible, which
  is indistinguishable from "focus silently failed". `focus.ps1` now walks the
  top-level windows instead, matching title **and** owning process - the title
  alone is not enough, because a Windows Explorer window showing a folder
  *named* EdenExplorer carries exactly that title. `click.ps1`, `drag.ps1` and
  `screenshot.ps1` still use the old `MainWindowHandle`/`MainWindowTitle`
  pattern and have the same blind spot when the window is minimized.

- **A self-update left the user with no program at all, because `restart()`
  spawned the replacement *before* the outgoing process had gone.** Reported
  as a dialog right after updating - "The running EdenExplorer window did not
  accept the request." - and the app simply never came back. `restart()` did
  `Command::new(exe).spawn()` and then `std::process::exit(0)`, so the new
  copy routinely started while the old one still held the single-instance
  mutex and still owned a window. The new copy's `acquire_or_forward` (the
  very first thing `main` does) therefore concluded "another instance is
  already running", found that window, and tried to hand its arguments over -
  to a process that was seconds from exiting. **There were two failure modes,
  and the quieter one was the worse of the two**, both reproduced here against
  the real unfixed binary: (1) if the outgoing window had already stopped
  pumping messages, `SendMessageW` returned 0 for two seconds of retries and
  the user got that dialog and no app; (2) if it was *still* pumping, the
  forward **succeeded** - so the replacement exited having handed its
  arguments to a program that then exited too, leaving **no window and no
  error at all**. A silent disappearance after an update, with nothing on
  screen to explain it. Fixed in two places: `restart()` now passes
  `launch::REPLACING_PREVIOUS` (`--replacing-previous`) and a copy started
  that way never forwards - it waits for the mutex and takes over; and
  `acquire_or_forward` no longer gives up when a forward is refused, waiting
  for the mutex and taking over instead. The second half can never produce two
  instances, because `CreateMutexW` decides who owns it rather than any guess
  about whether the other process still looks alive - verified by holding the
  mutex throughout and confirming the app still refuses to start. **The
  general lesson: a "hand my arguments to the instance that is already
  running" path has to know whether the instance it just found is the one it
  is replacing.** Finding a window is not evidence that anything is there to
  receive the message - and a forward that *succeeds* against a dying process
  is worse than one that fails, because it fails silently.
  **The flag alone was not enough, for a reason worth remembering whenever a
  bug is fixed in code that an *older* copy of the program is the one to
  run: the update that installs this fix is still performed by the version
  that has it wrong.** `restart()` belongs to the copy being replaced, so
  updating from a release that predates `--replacing-previous` still spawns
  the replacement without it - and failure mode (2) above would have bitten
  exactly once more, on the very update meant to fix it, looking for all the
  world like the fix had not worked. Covered by also having a *successful*
  forward that carried **no paths** watch briefly for the instance it just
  reached to disappear, and start up in its place if it does. Gated on there
  being no paths deliberately: `EdenExplorer.exe <folder>` means a tab was
  opened in a window the user is already looking at, and making every one of
  those linger ~1.6s to insure against a case that cannot arise there is a
  poor trade (measured: 153ms with the gate, 1779ms without).
- **A live test that matched this app's window by title alone reported "the
  app started" when what had actually appeared was the error dialog saying it
  refused to.** While reproducing the bug above, the first harness looked for
  a visible top-level window titled `EdenExplorer` owned by an
  `EdenExplorer.exe` process - exactly the rule `focus.ps1` uses - and
  cheerfully printed "NOT REPRODUCED: a window appeared". The window it found
  was `show_launch_error`'s own `MessageBoxW`, whose caption is also
  "EdenExplorer" and which is also owned by `EdenExplorer.exe`. The fix is to
  check the window **class** as well: a message box is class `#32770`, the
  real window is winit's own `Window Class`. This is the same shape as the
  documented screenshot gotchas - the check exits cleanly and the result looks
  entirely plausible - and it would have produced a confident, wrong "the bug
  does not reproduce" if the numbers had not been printed and read. When a
  test asserts "the app is up", assert on something a failure dialog cannot
  also satisfy.

- **The folder watcher added for custom context menu commands turned out to
  destroy any rename the user was in the middle of, including the one the app
  itself starts for a brand-new folder.** Reported as "when I create a new
  folder/file I don't get the chance to rename it, and the highlight stopped".
  `core::dir_watch` refreshes the view whenever a watched folder changes - and
  **creating a folder in that folder is itself a change it sees**, so the
  refresh fired a second or two after `create_new_folder` had opened the
  inline rename editor, re-listed the view, and took the editor's row out from
  under it. Two defects, not one, and only the first is about the watcher:
  (1) a refresh nobody asked for must not interrupt an edit in progress -
  `MainWindow::ui` now parks the change in `pending_dir_refresh` and applies
  it once `rename_state` is `None`, which costs nothing because committing or
  cancelling a rename reloads the view anyway; (2) `load_view_with_fallback`
  clears `selected_paths`/`selection_anchor`/`selection_focus`
  unconditionally, which is right when *navigating* and wrong when *refreshing
  the folder you are already in* - Explorer keeps the selection across a
  refresh. A new `refresh_view`/`refresh_path` pair captures the selection and
  restores it through `pending_selection_paths`, and F5, the toolbar button
  and the watcher all go through it now. It also has to read
  `pending_selection_paths` when `selected_paths` is still empty, since a
  just-created item is selected through the pending mechanism and the scan it
  is waiting on may not have finished. **The lesson is about background
  refreshes generally: anything that re-lists a view on its own schedule is
  invisible until it lands on top of something the user is doing, and the two
  things it can destroy are an in-progress edit and the selection.** If
  another automatic reload is ever added here, check it against both.
- **Two testing traps hit while verifying that fix, both of which produced a
  confident wrong answer first.** (1) The obvious check for "is the watcher
  armed?" - make an external change and see whether the view updates by
  itself - was run **70 seconds** after the last change, long past
  `dir_watch`'s 20-second `QUIET` timeout, so the watcher had already stopped
  and the probe proved nothing; the earlier "the rename survived, so the fix
  works" conclusion drawn from it was invalid, because the rename would have
  survived an unarmed watcher too. The sound version arms the watcher and
  probes **inside** the window, and proves the negative and the positive in
  one run: with no rename open the probe file appears on its own (watcher
  demonstrably live), with a rename open the same probe does *not* appear
  (deferral working), and ending the rename makes it appear (nothing stale).
  **A test for "feature X didn't fire" is worthless unless the same run also
  shows X firing when it should.** (2) A first attempt sent `Ctrl+Shift+N` to
  what turned out to be the **Settings** tab, which has no file list, so
  nothing happened and it looked like a failure - the window had restored its
  session with Settings active. Screenshot and confirm which tab is active
  before concluding a keyboard-driven test did nothing.

- **Making the sidebar's sections reorderable did not require moving any of
  them in the source, and trying to would have been the expensive mistake.**
  `draw_sidebar` is one ~1000-line chain of ten contiguous blocks, and the
  obvious way to make them orderable - cut each into a `match` arm - means
  relocating essentially the whole function. Wrapping each block in
  `if section == SidebarSection::X { ... }` inside a `for section in order`
  loop instead left every block exactly where it was: 33 inserted lines in a
  1660-line file, which is reviewable, where the `match` version would have
  been an unreadable diff. Ten cheap comparisons per pass cost nothing
  measurable next to the drawing itself. **Two things the loop does change,
  both of which the compiler catches:** a `let` in one block is no longer
  visible to the next (one hoist, `let mut places = places;`), and anything
  consumed by `if let Some(x) = opt` is *moved* on the first pass, so
  `folder_tree` became `.as_mut()` and two `Option` locals became `.take()`.
  If another sequential UI chain here ever needs to become orderable, reach
  for the guarded loop before the `match`.
- **Synthetic clicks died again mid-session, and this time the ui-test
  harness made it worse by clicking anyway.** A wrapper that tried
  `focus.ps1` a few times and then went ahead regardless posted a click while
  another window was foreground, and the screenshot that followed captured
  **this agent's own conversation window** - the exact trap already
  documented for `CopyFromScreen`, reached from a different direction. Any
  helper that focuses and then acts must *abort* when focus does not stick,
  and re-check focus again immediately before capturing, because a screenshot
  of the wrong window is entirely plausible-looking and worse than none. Once
  a control click (a large, unambiguous target that worked minutes earlier)
  also does nothing, clicks are confirmed dead for the session - stop, and
  verify by other means. Here that was enough anyway: the reorder had already
  been exercised by a real click, so the saved `sidebar_order.bin`, the live
  sidebar matching it, and the order surviving a restart proved the feature
  end to end without a single synthetic click landing.

- **Two releases in a row published with no program attached, every job went
  green, and the app told everyone they were running the latest.** Reported by
  the owner: "my current version is 1.4.1, but the latest release is 1.5.0 -
  it said you're running the latest update". The API showed why at once:
  v1.5.0 had **no assets at all** and v1.4.2 had **only its zip**, while
  v1.4.1 and everything before it had the full set. `core::release::newer_than`
  deliberately passes over a release with no `EdenExplorer.exe`
  (`exe: r.asset(EXE_ASSET)?` - there is nothing to install and nothing to
  check it against), so both were skipped and the check reported `UpToDate`.
  **The root cause is one missing line**: the publish step never set
  `$ErrorActionPreference = "Stop"`, so `Get-ChildItem release/dist -File`
  over a missing or empty directory wrote a *non-terminating* error and the
  upload loop then iterated over nothing at all. No throw, no failed step, and
  the metadata loop right after it still ran and set the title and notes
  correctly - which is precisely why the releases looked fine from the forge's
  own release page. **The deeper mistake is one this file already records and
  only half-applied:** after v1.0.2 came out with no notes, the lesson written
  down was "after any write through the forge's API, read the object back and
  assert on the fields you care about" - and that was done for the *title and
  notes*, which got a read-back-and-retry loop, while the *assets* stayed
  fire-and-forget. A rule applied to one half of a step is not applied. The
  publish step now sets `Stop`, refuses to publish unless `EdenExplorer.exe`
  and `SHA256SUMS.txt` are actually staged, and re-reads the release after
  uploading to confirm both are attached - re-uploading and finally failing
  the job if they are not. `core::update` also stopped reporting this as
  "up to date": a newer version that exists but cannot be installed is now
  its own `Event::Incomplete`, shown even for an automatic check, because a
  broken publish is otherwise invisible to everybody at once.
- **A workflow's PowerShell can be syntax-checked without running the
  workflow**, which is worth doing when the only other way to find out is to
  cut a release. Pull the step's `run:` block out of the YAML, replace the
  `${{ ... }}` expressions with a placeholder (they are not PowerShell), and
  parse it:
  `[System.Management.Automation.Language.Parser]::ParseFile(path, [ref]$null, [ref]$errs)`.
  That catches a typo in a step that would otherwise only surface after a
  merge to `main`, by which point it has already published something wrong.

- **A shell on a server reuses the whole terminal except the pipe, and that is
  the only part worth writing.** `alacritty_terminal`'s `EventLoop` is built
  around a local pseudo console, so there is nothing to reuse there - but the
  grid, the renderer, key encoding, selection and scrollback are all
  transport-agnostic. `core::ssh_shell` replaces just the loop: a thread reads
  a `russh` channel, feeds bytes through the same `vte::ansi::Processor` into
  the same `Term`, and sends keys and window-size changes back. `TerminalSession`
  grew a two-variant `Transport` and `Listener`'s reply path an `InputSink`;
  everything above them is unchanged. **Three things to know if this is
  touched again:** (1) `Processor::new()` needs its default type parameter
  named in type position (`let mut p: Processor = Processor::new()`), the
  default does not apply to the call; (2) `SshShell`'s `Drop` ends the shell,
  so the reader thread's reply path holds a separate send-only `SshSender`
  rather than a clone - a clone would kill the shell when either copy dropped;
  (3) it opens its **own** SSH connection rather than borrowing the SFTP one,
  because that one lives on a current-thread runtime that only advances inside
  a `block_on` per file operation, so a shell sharing it would only receive
  output while the user happened to be listing a directory. Both still sign in
  through the one `sftp::sign_in`.
- **A feature that needs a server can still be verified almost entirely
  without one, by testing the failure path on purpose.** The SSH shell could
  not be exercised end to end here - the local `sshd` is installed but stopped,
  and connecting to it would need the user's own Windows credentials, which is
  not something to handle. What *is* reachable without any credential is a
  refused TCP connect: seeding a connection in the dev profile's `ui_prefs.json`
  (JSON, so hand-editable, unlike the `postcard` files beside it) pointing at a
  port with nothing listening exercised the pane opening beside the files, the
  equal split, the profile naming, the transport thread starting, and the error
  arriving in the grid as readable text - everything except a successful
  sign-in. **Reach for the failure path when the success path needs
  credentials: it covers most of the same wiring and costs nothing.** Note the
  CLI cannot be used to navigate there - `launch::is_virtual_path` only accepts
  This PC and the Recycle Bin, so a `\\?\EdenRemote\...` argument is rejected
  as "not an existing directory"; the sidebar entry is the way in.

- **A key-value store can be browsed as a folder tree, but only because
  `RemoteFs` is small and its optional fields are genuinely optional.** Redis
  fits in seven methods over `/`-separated strings, and `RemoteEntry`'s `size`
  and `modified` are both `Option` - which is what lets a store with no
  modification time join at all. Keys split on `:` into folders, the same
  prefix-as-folder trick `s3` already plays. **Where it stops fitting is the
  part to keep in mind if this is ever extended:** only a `string` key has
  bytes (everything else is rendered as JSON for reading), `mkdir` has nothing
  to create because a folder is just a shared prefix in other keys' names, and
  `remove_dir`'s documented "removes an empty folder" has no equivalent at all
  - the operation that *would* make sense there is "delete every key under
  this prefix", which is a far more destructive thing wearing the same name.
  That is why the whole module is read-only: a file manager's reflexes are
  fast (select-all, Delete) and a live database has no Recycle Bin and no
  undo. If writing is added it needs its own confirmation, not the ordinary
  file gestures.
- **Two things about listing a keyspace that are easy to get wrong and cost
  real money on a production server.** (1) `KEYS *` blocks the server for the
  whole sweep - on a real instance that is an outage - so listing uses `SCAN`
  with a cursor and a `COUNT`, accepting that it is not a snapshot (a key
  added mid-scan may or may not appear, which for a folder listing is fine).
  (2) A folder's leaf keys each need a `TYPE`, and the string ones a
  `STRLEN`; done one at a time that is a sequential round trip per key, which
  over any real network turns a thousand-key folder from instant into
  unusable. They are **pipelined** instead - every request written, then every
  reply read - so a batch costs one round trip rather than N. RESP makes this
  trivial and it is the single biggest difference between this feeling fast
  and feeling broken.
- **A hand-rolled protocol client can be tested properly without the real
  server, by writing the server.** `core::remote::redis`'s tests start a
  `TcpListener` on port 0 that speaks just enough RESP2 - `PING`, `SCAN`'s
  cursor-plus-array reply, `TYPE`, `STRLEN`, `GET`, `HGETALL` - and drive a
  real `RedisFs` against it over a real socket. That covers the wire format,
  the cursor loop, the folder/leaf splitting at each level, the pipelined
  batch arriving in order, and the refusal of every write - none of which a
  pure unit test of the path helpers would reach, and all of which runs in CI
  with nothing installed. Prefer this to an `#[ignore]`d test needing a live
  service whenever the protocol is simple enough to fake.

- **Two performance mistakes of the same shape turned up in a sweep of
  recently-added features, and both were a convenience helper called from
  per-frame drawing code.** Neither is a hot loop; both are "ask a cheap
  question with an expensive call", repeated sixty times a second. (1) The
  SSH-shell layout asked `remote::connection_of(path).filter(|c|
  c.has_shell())` on **every frame**, in two places - and that allocates a
  `Vec<String>` of the path's segments *and* clones the whole
  `RemoteConnection` (six `String`s) to answer yes/no. Replaced by
  `shell_connection_of`, which parses only the leading id and returns it
  under one lock. (2) Worse and subtler: Settings > Network validated its
  selection with `form_for(..).is_none()`, and building a form for a remote
  location calls `credentials::load` - so merely *having that page open* read
  the **Windows Credential Manager on every frame**. Replaced by a
  `selection_exists` check that touches nothing. **The rule worth keeping:
  when adding a call to anything drawn every frame, look at what the helper
  does, not at what it returns** - `Option<u64>` and `Option<RemoteConnection>`
  read identically at the call site, and one of them is free. Both were found
  by reading the diff of the last few days' features for calls reachable from
  `draw`, which is a cheap sweep worth repeating after any run of UI work.
- **`echo test | gpg --clearsign` does not prove git can sign, because git
  may not be using that `gpg`.** After the recurring `signing failed: Timeout`
  came back, that probe kept reporting the key locked even once it was not -
  because `git config gpg.program` here is
  `C:\Program Files\GnuPG\bin\gpg.exe` while Git Bash's own `gpg` on PATH is
  MSYS2's `/usr/bin/gpg`: two installations, two agents, two separate
  passphrase caches. The probe (and any unlock instruction given to the user)
  has to name the same binary git uses - check `git config --get gpg.program`
  first - or the only reliable test is retrying the commit itself. This is the
  same split already recorded for `gpgconf --kill gpg-agent`, reached from a
  different direction.

- **The worst performance bug found in this codebase so far was a `metadata`
  call per item, per frame, in the status bar.** Counting a *folder* is free -
  `status_counts_for_view` reads `view.files`, already in memory - but the
  **tag view** has no `view.files` to read (it builds its own row list), so
  `status_counts_for_tag_group` walked the group's items calling
  `std::fs::metadata` on each. The status bar is drawn every frame, so a tag
  group of a hundred items meant roughly **six thousand filesystem calls a
  second** to keep one line of text up to date - and on tagged items living
  on a network path, each of those is a round trip. Fixed by memoizing on a
  hash of (group id, its items, the selection); hashing is `O(bytes)` where
  the old behaviour was `O(items)` *syscalls*. **The general lesson: when two
  code paths compute the same display value and one of them has no in-memory
  source for it, that path is where the syscalls hide.** Look for the
  fallback, not the main case.
- **Memoizing a displayed value is a correctness risk, so pin the signature
  with tests rather than the result.** A stale status bar showing the wrong
  item count is a worse bug than the cost it was avoiding. The two mistakes
  worth testing against specifically are a key built from *lengths* (which
  misses a different item being selected, or a group member being swapped for
  another) and one built from a `HashSet`'s iteration order (which has none,
  so the same selection can hash differently between frames). Both are pinned
  in `tag_counts_tests`.
- **A sweep of the whole app for per-frame work found three more of the same
  shape, all in the sidebar - which redraws unconditionally.**
  `dirs::home_dir()` asks the shell (`SHGetKnownFolderPath`) and allocates;
  the Administrative Tools entry read an environment variable, built a
  `PathBuf` and `is_dir`-ed it. Both answers are fixed for the life of the
  process, and both are now `OnceLock`s. **Everything else checked out**, and
  is worth recording so the next sweep can skip it: the COM/shell calls in
  `mainwindow_imp` are all in file operations rather than draw code,
  `icons.rs` is behind `DESKTOP_INI_CACHE` and the icon cache, drive
  enumeration has a 30-second cache, and `dir_watch`'s per-frame check is a
  relaxed atomic load. One known cost left alone deliberately:
  `status_counts_for_view` is `O(files)` per frame and makes a second full
  pass whenever anything is selected - real on a very large folder, but
  in-memory, and memoizing it safely needs an invalidation signal the view
  does not currently have (`view.files` has no generation counter), so
  inventing one would mean touching every site that mutates it.

- **`Response::hovered()` is false for every widget under the pointer except
  the top-most one, so using it as a drop-target fallback silently loses the
  drop whenever the release lands on a row.** Reported against spring-loaded
  folders: "when I select multiple files, and then drag on a folder, the
  folder opens, and then when I release the mouse, the files are not moving
  into the folder and the mouse cursor stuck on drag cursor icon." The file
  list's drop handler resolves its target as "the folder row under the
  pointer, or else the folder being shown", and that second half was written
  as `bg_response.hovered().then(|| current_dir.clone())`. egui's own
  `InteractionSnapshot` builds `hovered` from `hits.click`/`hits.drag` only -
  the single top-most interactive widget - and documents it as "while dragging
  or clicking something, nothing else is hovered". The background therefore
  reads as un-hovered whenever *any* row is under the pointer, and a row that
  isn't a folder sets no target of its own, so the drop resolved to `None` and
  did nothing at all. Ordinary drags mostly hide this (people aim at the
  folder, or at empty space below the list); a spring-open exposes it every
  time, because the pointer is left sitting at the screen position the folder
  row occupied, which in the folder that just opened is usually a file.
  **`contains_pointer()` is the one that means "the pointer is inside this
  rect"** - use it for drop zones (egui's own doc comment on `contains_pointer`
  says exactly that), and keep `hovered()` for highlight/tooltip decisions.
- **The same bug's other half: every view's drop handling lives inside its
  "there are rows to draw" branch, so an empty folder swallows the release -
  and that is worse than losing the drop, because it also never ends the
  drag.** This is the third time this file has recorded the same shape of
  gap (the double-click-to-go-up handler, then the empty-folder context
  menu), and the cost is higher here: `drag_state` is what the ghost label
  is drawn from, so a release the view never saw leaves it following the
  pointer for ever with Escape the only way out. A spring-opened folder is
  *always* in this branch for the first frames after it opens (the scan is
  async and `files` is cleared on navigation), so it is reachable even for a
  folder that has content. Fixed with a shared
  `handle_drop_on_empty_view` used by all three empty branches, **and** with
  a frame-level reset in `MainWindow::ui` - if the primary button was
  released this frame, the drag is over, whichever branch happened to draw.
  Prefer that belt-and-braces shape for any "this gesture has ended" state:
  per-handler clearing is what makes the gesture *do* something, a single
  end-of-frame reset is what guarantees it stops.
- **Synthetic drag-and-drop *is* testable here after all - with `SendInput`,
  not `PostMessage`.** This file already records that a `PostMessage`-built
  drag could not be made to register, and concluded drag-and-drop was
  "unverifiable via this skill's current scripts". That conclusion was about
  `PostMessage` specifically: `SendInput` with
  `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK` injects at the OS input
  layer, so winit and egui see an ordinary mouse, and a multi-level
  spring-loaded drag (press, glide, dwell 1.4s, dwell again, dwell again,
  release - three folders deep without letting go) drove the real app
  perfectly. Three things it needs: (1) **PowerShell cannot assign to a
  nested struct field** - `$i.u.mi.dx = 5` silently does nothing because
  `$i.u` hands back a copy, so build `MOUSEINPUT`, then `UNION`, then
  `INPUT` and assign each whole; (2) call `SetProcessDpiAwareness(2)` first
  or every coordinate is virtualized; (3) check `SendInput`'s return value -
  a struct-size mismatch fails silently otherwise. The harness
  (`win.ps1`/`input.ps1` in that session's scratchpad) re-checks
  `GetForegroundWindow`'s owning **pid** before every single event and aborts
  if it isn't the target, which is what kept real input from landing in the
  owner's own window when they came back to the keyboard mid-test.
- **A debug build can be driven safely alongside the owner's own running
  copy**, which is what made the above testable at all without closing
  theirs: launch it with `--new-window` (`main.rs` skips the single-instance
  mutex entirely for that flag, so it neither forwards to nor fights the
  running instance) and create `target/debug/EdenExplorerData` first, which
  puts it in portable mode against its own settings folder rather than the
  real `%LOCALAPPDATA%\ExplorerEden`. Redirect its stderr to a file and a
  debug build's `eprintln!`s are readable without a console. Find its window
  by **owning pid**, never by title - both processes are called
  `EdenExplorer.exe` and both windows are titled `EdenExplorer`.

- **A read-only database connection is not enough to leave a folder alone, and
  that is the thing to check before calling any viewer "read-only".** The
  SQLite view opens every connection `SQLITE_OPEN_READ_ONLY` with
  `query_only` set, and a test pins that a direct `INSERT`/`DELETE`/`DROP`
  through it fails - all correct, and all beside the point: a database in WAL
  mode needs a shared-memory index to be read at all, so SQLite *created*
  `app.db-shm` and an empty `app.db-wal` next to the file. Merely looking at
  someone's database wrote two files into their folder. Found by a test that
  listed the directory before and after a read and compared, which is worth
  writing for anything claiming to be read-only - the connection flag only
  says what *this* code may do, not what the library does underneath it. The
  fix is `immutable=1` in the open URI, which tells SQLite the file cannot
  change and makes it read the main file directly, touching nothing. It is
  only correct when nothing is pending, though: with a `-wal` present,
  `immutable=1` silently shows stale content, so the open checks for
  `-wal`/`-journal` first and falls back to the ordinary read-only open (where
  the sidecars already exist anyway, because whatever wrote them made them).
  Both halves need their own test, or a later change will quietly trade one
  for the other.
- **Reach for the real library over hand-rolling a format when the format has
  a mode that hides data outside the file.** This codebase hand-rolls RESP2,
  NBSTAT, mDNS and WS-Discovery rather than pulling in a client crate, and the
  same instinct for SQLite would have been wrong: a reader built on the
  (well-documented, stable) file format would be correct for a database at
  rest and silently wrong for any database in WAL mode, which is most of the
  ones a person actually meets - Firefox, Chrome, Electron apps. "Silently
  shows some of the rows" is a worse failure than not having the feature. The
  C dependency that decision implies cost nothing new here, which is worth
  checking before arguing about it: `cc` and `ring` were already in the tree,
  so the build already compiled C and assembly.
- **`Context::fonts` hands out a shared `&Fonts`, which cannot lay anything
  out - measuring text needs a `Painter`, and therefore a `Ui`.** Fitting the
  SQLite grid's columns to their content wanted a measurement at the moment a
  page of rows arrived, which happens in a channel-polling function that only
  has a `Context`. `ctx.fonts(|fonts| fonts.layout_no_wrap(...))` does not
  compile (`layout_no_wrap` takes `&mut self`), and neither does `layout`.
  The shape that works is to have the load *invalidate* the widths and let the
  next draw fit them, where `ui.painter().layout_no_wrap(...)` is available -
  still once per load, since the fitted widths then match the column count and
  the branch is skipped.
- **The debug build's own console window is enough to break every synthetic
  input test, and it looks exactly like the long-documented focus-stealing
  gotcha.** A `--new-window` debug instance kept losing the foreground between
  `focus` and the first click, including when both ran in one PowerShell
  process (the fix already recorded for this symptom). The thief was the
  console that debug builds keep attached: it belongs to a *different* process
  (conhost), so a check that only compares the foreground window's pid against
  the app's reports a stranger and aborts. Use a **release** build for any
  live input test that doesn't need `eprintln!` output - `windows_subsystem =
  "windows"` means no console and no thief. A related trap in the same
  harness: an enumerator that picks "the first visible non-`#32770` window
  owned by this pid, wider than 400px" picks a *tooltip* once one is showing,
  so match on the window text as well.
- **An unrecognised double-click target can leave a modal "Open With" dialog
  sitting in the foreground, which then blocks every later test and looks like
  the app stopped responding.** Live-testing the SQLite view meant
  double-clicking `.db` files, and the deliberately-planted decoy (a text file
  named `notes.db`) correctly fell through to Windows - which answered with
  its "How do you want to open this file?" picker. Two of those accumulated
  before the cause was obvious, because the only symptom was
  `GetForegroundWindow` returning an unfamiliar pid. `OpenWith.exe` performs
  no action unless a program is chosen, so terminating that process is the
  cancel-equivalent and is far safer than clicking inside it (see the entries
  above about native dialogs and `{ESC}`). Worth knowing before testing any
  "open this file type" behaviour: the failure path *is* an OS dialog, and it
  is modal over everything.

- **A heading containing `&` cannot have one anchor link that works on both
  GitHub and Forgejo, and this repo is read on both.** The README's new
  feature index linked to `#browsing--navigation` (GitHub's slug for
  "Browsing & Navigation": the ampersand is dropped and *both* of its spaces
  become hyphens). Forgejo slugs the same heading `browsing-navigation`,
  collapsing it to a single hyphen - so every one of the sixteen index links
  was dead on the instance that actually hosts the repo. Found by reading the
  rendered heading ids back off the live page
  (`[...md.querySelectorAll('h3')].map(h => h.id)`) rather than trusting the
  slug rule, which is the only way to tell two renderers apart. Fixed by
  spelling the headings "and" instead of "&", which both renderers slug
  identically. **More generally: an in-page anchor is renderer-specific, so
  verify one against the renderer that will actually serve the file** - and
  prefer heading text that contains nothing but letters, digits and spaces,
  which is the only input the two agree on.
- **Markdown inside a raw HTML block is not rendered by GitHub or Forgejo.**
  The same index started life as an HTML `<table>` with markdown links and
  bold in its cells, which renders as the literal text `**Core**` and
  `[Browsing & Navigation](#browsing--navigation)`. This is why every
  centred block in this README is written in HTML (`<b>`, `<a href>`) rather
  than markdown - a convention worth following rather than rediscovering.
  Caught by rendering the file and looking at it, not by reading it.

- **A list that is drawn grouped but stored flat needs its "move up" to mean
  "past the previous item of my own group", not "past my neighbour".** The
  network locations live in one `Vec<RemoteConnection>` that the sidebar
  filters per protocol, so an SFTP connection can sit next to an FTP one in
  the list while appearing three rows apart on screen. Swapping with the
  immediate neighbour would reorder the storage and change nothing visible -
  the classic version of this bug, and the reason `move_connection` searches
  for the nearest entry of the same `kind` in the chosen direction instead.
  Worth checking for whenever reorder arrows are added to anything whose
  display order is a *filtered view* of its storage order.
- **Reorder arrows that act on a filtered view must write back into the
  unfiltered one.** Settings > Network only draws a protocol group that has
  locations in it, so the arrows move a group among the *shown* groups - but
  the saved order contains every group, including empty ones. Moving the
  shown list and then writing it back over only the shown positions keeps an
  empty group where it was; naively saving the shown order would silently
  drop every empty group to the end. Verified by moving Redis above S3 and
  checking that the empty `ftps` group had not moved in `ui_prefs.json`.
- **`MOUSEINPUT.mouseData` is an unsigned field carrying a signed value, so a
  scroll-down delta has to be reinterpreted rather than cast.** Adding wheel
  support to the ui-test harness, `[uint32](-960)` throws
  "Value was either too large or too small for a UInt32" in PowerShell;
  `[BitConverter]::ToUInt32([BitConverter]::GetBytes([int]$delta), 0)` is the
  conversion that actually means "the same bits". Wheel input is worth having
  in that harness: scrolling a pane into view with a coordinate-guessed drag
  is the single most frequently recorded way a session in this repo has
  clicked something real by accident.

- **A document store maps onto a folder tree far more naturally than a SQL
  database does, and the win is that a document really is a file.** CouchDB's
  databases become folders and its documents become `<id>.json` files, which
  means the preview pane, Quick Look and syntax highlighting all work on them
  with no new code - they dispatch on the extension, which is the whole reason
  the extension is added rather than showing the bare id. The name has to
  round-trip, though: `id_from_file_name` strips exactly *one* trailing
  `.json`, so a document genuinely called `notes.json` lists as
  `notes.json.json` and still resolves. If another document store is added,
  copy that rule rather than inventing a second one.
- **`_design/x` is one CouchDB id containing a `/`, and remote paths here are
  split on `/` - so the prefix has to become a folder or the id is destroyed.**
  Treating `_design` and `_local` as levels of their own is both forced by the
  path model and what CouchDB's own interface does, and the real id is
  reassembled (`_design/views`) when the document is fetched. Pinned by a test
  asserting the request path, because a URL-encoded `%2F` would look right in
  a listing and 404 on open.
- **Page an HTTP listing by key, not by offset, whenever the server offers
  it.** CouchDB's `skip` walks every row it skips, so offset paging gets
  slower the deeper into a database it goes, while `startkey` is a seek into
  the B-tree. The client asks for `limit = PAGE + 1` and treats the extra row
  as the *next* page's first - which is exactly the place an off-by-one
  duplicates one document per page, so the test asserts no name appears twice
  rather than only checking the count.
- **A stand-in server in the test must honour the request, or it tests
  nothing.** The first version of the CouchDB fake returned a fixed number of
  rows regardless of the `limit` asked for, so the client's "I got back as
  many as I asked for, so there is more" rule never fired and the paging test
  passed against a single page. Making the fake respect `limit` - and driving
  the real `PAGE` constant with enough documents to need three round trips -
  is what actually exercised the loop. Worth remembering for any hand-written
  test double: the parts of the protocol it ignores are the parts it cannot
  test.

- **Whether to hand-roll a protocol or take the library turns on one
  question: can the data live somewhere the naive reader won't look?** This
  codebase hand-rolls RESP2, NBSTAT, mDNS, WS-Discovery and now MongoDB's
  OP_MSG, but deliberately took bundled SQLite. The difference is not size or
  difficulty - it is that a SQLite database in WAL mode keeps recent rows in a
  *separate file*, so a hand-written reader is silently wrong for most real
  databases, while MongoDB's server simply reports what it has and the things
  that can go wrong (framing, auth) fail loudly and immediately. Apply that
  test rather than a general preference next time: silent wrongness buys the
  dependency, loud failure does not.
- **An authentication protocol is only testable against its own published
  vectors.** SCRAM-SHA-256 has four places to be subtly wrong - the PBKDF2
  block, the two key derivations, the exact transcript string, and the XOR -
  and every one of them produces a self-consistent implementation that no
  amount of round-tripping against itself would catch. RFC 7677's worked
  example (user `user`, password `pencil`, a fixed nonce) pins the
  client-first message, the client proof and the server signature
  byte-for-byte, and it found nothing only because it was written first. Do
  the same for any crypto-adjacent protocol here; a stand-in server that
  reuses the client's own code proves nothing on its own.
- **Verify the server's half of a challenge-response too.** SCRAM ends with
  the server proving *it* knew the password, and skipping that check is easy
  because everything works without it - a server that never knew the password
  would still be accepted. The test that matters is the one that signs with
  the right password and connects with the wrong one, asserting the client
  refuses.
- **A test double that reuses the code under test can only test the parts it
  doesn't reuse.** The MongoDB stand-in builds its own OP_MSG headers by hand
  rather than calling the client's framing, so a mistake there can't cancel
  itself out; it does use the BSON codec, which is acceptable only because
  that codec is separately pinned against hand-built bytes and known vectors.
  Decide per layer which half of the pair is allowed to be shared.

- **Page a search engine by cursor, not by offset - the offset has a hard
  ceiling, not just a slope.** Elasticsearch refuses `from + size` past
  `max_result_window` (10,000 by default) outright, so an offset-paged listing
  doesn't merely get slow on a large index, it stops returning anything. The
  scroll API is the cursor equivalent, and it has an obligation attached:
  every open scroll holds search context on every shard until it expires, so
  it is cleared with a `DELETE` when the walk ends. The test asserts all
  three - that the first request opens a scroll, that later ones follow it,
  and that it is given back - because the cleanup is the part that works
  silently either way.
- **When an id can contain the path separator, escape it, don't sanitise
  it.** Elasticsearch document ids may contain `/`, and paths in this app are
  split on `/`. Replacing it (the way `mongodb` does for ids it can only
  match client-side) invents a folder level *and* loses the real id, so the
  document can't be fetched back. Percent-encoding `/`, `\\` and `%` keeps the
  name one segment and perfectly reversible - a round-trip test over
  awkward ids is what pins it, and it caught nothing only because it was
  written first.
- **Compute a test's expected value independently, not by running the code
  and copying what it printed.** The `@timestamp` test asserted that
  `2026-10-10T08:30:00Z` is epoch 1,791,707,400; the code said
  1,791,621,000 and the code was right - the hand-written constant was a day
  out. The temptation at that point is to paste in whatever the code
  produced, which turns the test into a tautology. Checking the date
  separately confirmed which side was wrong, and the constant is now derived
  once and reused across all three assertions.

## Build & release workflow

CI/CD runs on the **Forgejo** instance that hosts this repo (`git.zacx.net`), not
GitHub - see the recorded finding above for why nothing here may use a published
action. Workflows live in `.forgejo/workflows/`.

### The runner

One runner serves every `owner/*` repository on the forge, including
`flex-taskbar`. It is a Windows service on the development machine itself
(`gitea-runner`, NSSM-wrapped, `C:\Apps\gitea-runner`, running as
LocalSystem), registered at account scope rather than per repository - there
were briefly two repo-scoped runners, which let the two projects build
concurrently and defeated the single-runner serialization both their workflows
assume. `C:\Apps\gitea-runner\make-global.ps1` is what collapsed them.

- **A runner service needs `cargo`/`rustup` put on its PATH explicitly.** They
  live in `C:\Rust\.cargo\bin`, which is on the *user* PATH only, and a
  service running as LocalSystem sees just the machine PATH - which does have
  `git` and `pwsh`, so the checkout and the version plan both succeed and the
  job then dies at `rustup update stable` about eight seconds in. The working
  service supplies a full PATH through NSSM's `AppEnvironmentExtra`; a runner
  installed with a plain `nssm install` has none and fails exactly that way.
  If a new runner is ever set up, copy that value from the existing service
  rather than retyping it.
- `runs-on: windows-latest` matches because the runner registers the label
  `windows-latest:host` - `host` meaning jobs run directly on the machine, not
  in Docker, which is what a Rust build here needs.
- There is no `unregister` command. A replaced registration leaves a stale
  runner entry that reads "offline" for ever in the forge UI; delete those by
  hand.
- Re-registering deletes `.runner`, and a bad token then leaves the machine
  with no runner at all and both projects unable to build - back that file up
  before touching it (`make-global.ps1` does, and restores it on failure).

### Workflow

- After a change is tested (local `cargo test` + `cargo clippy --release
  --all-targets --locked`) **and** the documentation (`README.md`, `CHECKLIST.md`,
  `SUMMARY.md`, and this file) is updated to match, merge the branch into `main`
  and push `main`. **The repository owner asked for this to be done every time
  without asking again.**
- **Never merge while CI is red.** Push the branch first, let
  `.forgejo/workflows/ci.yml` run on it, and merge only once it is green.
- `ci.yml` deliberately does **not** run on `main` - the release workflow runs
  there, and the instance has a single runner whose working copy every job empties
  on entry, so two workflows on one commit delete each other's files mid-build.
  A branch is therefore the only place a change gets checked before it lands.

### Checks

- `cargo test` - the real gate. 300+ tests, all platform-independent logic.
- `cargo clippy --release --all-targets --locked` - **without** `-D warnings`.
  This codebase currently carries ~300 clippy warnings; denying them would fail
  every run. Real compile errors still fail. Don't add `-D warnings` without
  clearing the backlog first.
- `cargo fmt --all --check` - **advisory only, and it currently fails.** This
  codebase has never been rustfmt-formatted, so both workflows report the drift
  as a warning rather than failing. Running `cargo fmt --all` would touch
  essentially every file; treat that as its own deliberate one-time change, not
  something to slip into an unrelated commit.
- CI runs the same three on the Forgejo Windows runner with the latest stable
  Rust, which can have newer clippy lints than a local toolchain.

### Releases

- `.forgejo/workflows/release.yml` publishes a release on **every push to
  `main`** - there is no manual version bump and no tag to push by hand. It works
  out the version from the commits since the last `v*` tag, re-runs the checks,
  bumps `Cargo.toml` + `Cargo.lock`, builds, tags the bumped commit, and
  publishes.
- **Commit subjects decide the version**, so they matter: `Add ...` (also
  `Introduce`/`Implement`/`Support`/`Redesign`/`Enable`/`feat:`) is a **minor**
  bump, `Fix ...` and any other verb a **patch**, a `BREAKING CHANGE:` footer or
  `feat!:` a **major**, and `Document ...`/`docs:`/`chore:`/`ci:`/`style:`/
  `test:`/`refactor:`/`Merge ...` cut **no release at all**. A push whose commits
  are all in that last group publishes nothing and stops, which is what keeps a
  documentation-only push from shipping an update to everyone. Run
  `pwsh -File scripts/next-release.ps1` to see what the next release would be
  without changing anything.
- **Never hand-edit `version` in `Cargo.toml`.** The workflow owns it
  (`scripts/update-version.ps1` moves it and `Cargo.lock` together, which
  `--locked` requires). The tag and the `Cargo.toml` at that tag always agree by
  construction, because the bump is committed *before* the build and the tag
  points at that commit - and they have to agree, because the app compares a
  release's tag against the version it was built with, so a mismatch would offer
  the same update for ever.
- **Never publish the program without `SHA256SUMS.txt` beside it** - the app
  refuses to install a release it cannot check, and only offers its page instead.
  `scripts/build-and-package.ps1` always writes both, plus a versioned zip.
- The published asset names are a contract with `core::release`/`core::update`,
  not a convention: `EdenExplorer.exe` (downloaded in place - a versioned filename
  would break the updater), `SHA256SUMS.txt`, and
  `EdenExplorer-<version>-windows-x64.zip`. Tags are plain `vMAJOR.MINOR.PATCH`;
  anything after the three numbers (`-rc1`) is not a version the updater parses.

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