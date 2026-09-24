# Recent Features & Fixes

A day-by-day log of what's been added or fixed recently. For the full
feature list, see [README.md](README.md#-features); for phase-by-phase
delivery tracking, see [CHECKLIST.md](CHECKLIST.md).

## 2026-09-24

- **Reordered the file/folder context menu**: Send To now sits right below
  "Open In Default Program", and Compress (Zip) now sits directly above
  the Custom Context Menu section - both were previously much further
  down the menu.

- **Title-cased a batch of context-menu and tab-menu labels** that were
  showing up in sentence case or with lowercase shortcut hints - "Pin tab"
  → "Pin Tab", "Add to Favorites" → "Add To Favorites", "Open in split
  view" → "Open In Split View", "Compress (zip)" → "Compress (Zip)",
  "Cut/Copy/Paste (ctrl+x/ctrl+c/ctrl+v)" → "(Ctrl+X)/(Ctrl+C)/(Ctrl+V)",
  and several more of the same shape.

- **Fixed: a network PC whose shares couldn't be listed (a permissions
  problem on that server - Guest/anonymous access disabled, or requiring
  credentials this PC doesn't have) showed the exact same "This folder is
  empty" text as a server that genuinely has zero shares**, making a real
  access problem look like nothing was ever shared. Opening `\\server` now
  shows "Access denied - this PC doesn't have permission to list this
  server's shares" (or a generic failure message) instead, whenever share
  enumeration actually fails.

## 2026-09-23

- **Fixed: the shared glyph icon picker (used by both Send To and Custom
  Context Menu) showed a visibly different list height depending on which
  page opened it**, even though both call the exact same function. The
  icon grid's `ScrollArea` capped its height at 280px but never guaranteed
  it - if the popup happened to open with less room below it, it silently
  shrank instead. Reserved the height explicitly beforehand so the grid
  now renders at a consistent height everywhere it's opened from.

## 2026-09-22

- **Send To: "Replace All"/"Skip All"/"Rename All", and per-group Copy or
  Move.** When a Send To batch (copying/moving into several folders at
  once) collides with an existing name in more than one destination, the
  conflict dialog now offers "Replace All"/"Skip All"/"Rename All" - pick
  one once and it applies automatically to every remaining destination in
  that batch, instead of clicking Replace/Skip/Rename separately for each
  one. Separately, each Send To group can now be set to Copy or Move in
  Settings (right below its icon), and the right-click menu now reflects
  it: "Send To" opens into "Copy" and "Move" branches, each listing only
  the groups configured for that operation.
- **Send To moved to between Delete Permanently and Create Shortcut** (was
  between Compress and Cut), and the "Windows menu items" entry at the
  bottom of the right-click menu now reads a plain, static "Windows Menu
  Item" instead of switching between "Show"/"Hide Windows menu items" text.
  It was already hidden whenever "Enable Windows context menu items" is
  turned off in Settings - that part needed no change.
- **New "Send To" feature.** A new Settings page (between Custom Context
  Menu and Tab Groups) for creating named groups of destination folders,
  each with its own icon (a Phosphor glyph or a browsed image file) and an
  ordered, reorderable list of folders. A master toggle - only usable once
  at least one group exists - enables a single "Send To" entry in the
  file/folder right-click context menu, positioned between Compress (zip)
  and Cut with its own separators: it opens into each of your groups (shown
  with its own icon), and clicking a group copies the current selection
  into *every* folder in it at once - right-click → Send To → My
  Destinations copies straight to all of My Destinations' folders in one
  action, no per-folder picking required. Always a copy, never a move - the
  originals stay put no matter which group is picked. A name collision at
  any one destination gets the same Replace/Skip/Rename/Cancel prompt as a
  regular paste (queued one destination at a time if more than one
  collides), and each destination gets its own entry in the notification
  panel and its own completion toast, exactly like any other copy
  operation. Two bugs found and fixed via direct follow-up feedback while
  building this: the context-menu-enable checkbox initially rendered as a
  nearly-invisible dot far from its label and couldn't be clicked (fixed by
  routing it through the same bounded-checkbox helper every other Settings
  checkbox uses), and the menu itself first shipped as groups appearing
  directly as top-level entries with each one opening into a per-folder
  picker - corrected to the single-click, copy-to-every-folder shape
  described above, with each group's own icon now shown in the menu too.
- **Custom Context Menu also gets an enable/hide toggle**, matching Send
  To's: usable only once at least one custom command or submenu exists, and
  automatically hidden from the right-click menu whenever the list is
  empty - previously, any saved command always showed up in the menu with
  no way to temporarily turn the whole feature off without deleting
  everything.
- **"Create Shortcut" context menu entries, like Windows.** Right-click a
  file or folder for a new "Create Shortcut" entry - grouped with Checksums
  and Properties as its own section, separated from Delete by a divider -
  that creates a real `.lnk` next to it (`"<name> - Shortcut.lnk"`,
  auto-numbered on a collision, same as Explorer). Right-click empty
  background space in a folder for the same entry alongside New Folder/New
  File; it prompts for a target file via a native picker, then creates the
  shortcut in the current folder. Both paths select the new shortcut
  afterward.
- **New "Administrative Tools" sidebar link** (Places section). Points at
  the real folder Windows keeps every admin snap-in shortcut in
  (`%ProgramData%\Microsoft\Windows\Start Menu\Programs\Administrative
  Tools` - the one that actually holds the shortcuts, since the per-user
  equivalent location is typically empty), so Computer Management, Event
  Viewer, Services, Task Scheduler, Registry Editor, and the rest are one
  click away and open exactly like any other file. Only shown if that
  folder actually exists on the machine.
- **Tab Groups now support dual-pane entries.** A saved group entry can pair
  two folders - opening it restores a dual-pane tab with both folders side
  by side, not just one. Build a dual-pane entry automatically by
  right-clicking a tab that already has a split view open and choosing "Add
  to New/Existing Group," or add one by hand from the Tab Groups settings
  page (each folder row gets its own "Add Split Pane" button).
- **Fixed: opening a dual-pane group entry showed "this folder is empty" in
  the second pane**, even though the real folder had files - the split
  pane's file list was never actually loaded, only its path was set. Fixed
  so both panes load correctly the moment the group opens.
- **Fixed: copying, moving, or resolving a paste conflict (Replace/Skip/
  Rename) into the other pane in dual-pane view snapped focus back to the
  first pane.** Two separate causes, both fixed: the app was restoring
  whichever pane was focused *before* the action instead of keeping focus
  on the pane you just interacted with, and the paste-conflict dialog's own
  buttons (centered on the whole window, not either pane) could
  accidentally re-trigger the app's raw click-position pane-focus
  detection.
- **Fixed: exporting/importing settings (and every other file/folder
  picker) could briefly show glitched window chrome** - a bare system
  Close button and a stray "?" icon in place of the app's own themed title
  bar - while the dialog was open. Every native dialog in the app is now
  properly parented to the main window.
- **Notification settings reworked into two independent toggles**
  (Settings → Behavior): whether starting an operation automatically opens
  the notification bell's panel, and whether toast popups show at all. The
  bell icon itself is always visible now - an earlier version of this
  setting that hid it entirely was rolled back based on feedback.

## 2026-09-21

- **Fixed per-file icons for `.exe`/`.dll`/`.ico`/`.lnk`/`.scr`/`.cpl`
  files.** These extensions carry a unique icon baked into each individual
  file, but the icon cache was keying and looking them up by extension
  alone (the same fast path used for `.txt`/`.pdf`/etc., which do share one
  icon) - so every file of one of these types could end up showing the same
  wrong or missing icon. Each file now resolves and caches its own real
  icon.
- **The "+" (add-tab) button now defaults to "This PC."** A plain click
  used to duplicate whatever tab was currently open; it now opens "This PC"
  directly. Right-clicking it shows a menu with an explicit "Open New Tab
  (This PC)" entry plus every saved Tab Group, for opening or replacing
  your current tabs with one in a single click.

## 2026-09-20

- **Export Theme now bundles your Custom Themes list**, not just the
  active palette; importing merges custom themes by name (overwriting an
  existing same-named theme in place) instead of creating duplicates.
- **Fixed the type-to-filter box matching letters out of order.** Typing
  "te" used to match names like "Make The Doc" or "Fast Meet" that don't
  actually contain "te" as a substring - it was a fuzzy-finder-style
  subsequence match, better suited to a command palette than a plain
  filename filter. It's now a predictable, plain substring match.
- **Custom Themes editor layout finalized**: Primary and Secondary color
  pickers plus the Swap Colors button laid out as a compact 2-column grid
  that hugs its own content, with a clear name field that resets when you
  pick a prebuilt theme instead.

## 2026-09-19

- **Custom theme creation live-edits your Primary/Secondary colors** as you
  pick them, instead of only taking effect once saved.
- **Precise Primary/Secondary-driven theming**, following an explicit
  derivation spec: badges, checkbox hover/active, the search box's active
  icon background, Settings navigation text/icon, pinned tabs, and
  notification/toast borders now genuinely derive from your accent and
  secondary colors (some previously didn't derive from either, or derived
  incorrectly).
- **Swap Colors button** added to the Custom Themes editor's Primary/
  Secondary row, after several rounds of exact layout feedback; the Custom
  Themes list itself is now left-aligned with full (untruncated) names and
  wider gaps between entries.
- **Your last-selected custom theme is now remembered across restarts.**
- **19 Color Scheme swatches** to start a theme from (8 more added).
- **Notification and toast borders are more visible** (raised alpha),
  retroactively applied to already-saved custom themes via an automatic
  one-time migration, not just newly-created ones.
- **You can now add an item that already has a tag to a second tag group**
  - this was previously blocked once an item had one tag.
- **Paste-conflict dialog no longer freezes** when clicking Replace or
  Rename - the slow work (Recycle Bin enumeration, folder-size
  calculation) now runs on a background thread instead of blocking the
  frame the dialog needed to disappear on. A toast now also appears the
  moment a copy/move/delete/rename *starts*, not only when it finishes, and
  the notification panel auto-opens when an operation begins.

## 2026-09-18

- **Empty-folder right-click context menu.** Right-clicking empty space in
  an *empty* folder now shows New Folder/New File/Paste, the same as
  right-clicking empty space in a populated one. A newly created item is
  automatically scrolled to and selected - fixed to work correctly even in
  a folder with thousands of files still being scanned.
- **User-configurable minimum tab width** (Settings → Appearance → Layout),
  with an instant reflection in the Live Preview panel.
- **Fixed Phosphor icons rendering as wrong (CJK) characters** on systems
  where the default system font had been replaced by a large merged font
  build that happened to redefine the same private-use Unicode codepoints
  Phosphor's icon glyphs live at.
- **Circular count badges** added across the Custom Context Menu, Tab
  Groups, and Tags settings pages, plus a new themeable **Badge Color**
  used by the notification bell's own count badge.
- Assorted layout polish: fixed inconsistent gaps between master-detail
  settings list rows, fixed row icons/labels swallowing clicks meant for
  the row itself, gave the Live Preview panel's Notifications & Status its
  own row, and cleaned up several Gallery-view toolbar/divider spacing and
  alignment issues (including a stray line trailing the Tags column in the
  Details table).

---

*For the complete, always-current feature list, see the "✅ Implemented
Features" section in [README.md](README.md). This file only tracks recent
day-to-day additions.*
