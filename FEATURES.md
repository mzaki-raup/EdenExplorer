# Recent Features & Fixes

A day-by-day log of what's been added or fixed recently. For the full
feature list, see [README.md](README.md#-features); for phase-by-phase
delivery tracking, see [CHECKLIST.md](CHECKLIST.md).

## 2026-09-22

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
