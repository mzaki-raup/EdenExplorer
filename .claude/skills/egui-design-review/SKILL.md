---
name: egui-design-review
description: Review a UI/UX change to EdenExplorer for visual and interaction consistency with the rest of the app - spacing/padding, theme colors (light AND dark), icon choice, native-Explorer interaction parity, and i18n - before calling the change finished. Use this whenever you (or the user) just implemented or modified anything visual (a new view/panel/dialog, a context menu, a table's columns, spacing around a widget, a new icon or color) and are about to report it as done, or when the user asks you to "review the design", "check the UI/UX", "does this look consistent", or "polish" a feature. This is a consistency/polish pass, not a bug hunt - use /code-review or the built-in code-review skill for correctness issues instead.
---

# EdenExplorer design review

A UI change that compiles and "looks fine" in isolation can still be wrong
relative to the rest of the app - inconsistent padding, a hardcoded color
that breaks in the other theme, an icon that doesn't match the app's
existing vocabulary, or an interaction that doesn't match what native
Windows Explorer (and the rest of EdenExplorer) already does for the same
kind of action. This skill is a checklist for catching that class of
problem before calling a visual change done.

## Process

1. **Identify what actually changed.** `git diff` (or the specific files
   you just edited) - don't review the whole app, review the delta.
2. **Walk the checklist below** against the changed code, file by file.
3. **Look at it running, in both themes.** Use the `eden-explorer-ui-test`
   skill to build, launch, and screenshot the change. Check both light and
   dark - a hardcoded color or a palette field used inconsistently often
   only shows up in one of the two. (Theme is switched from the app's own
   Settings > Appearance page, or by toggling the OS theme if the app
   follows system theme.)
4. **Report findings** as a short list: file/line, what's inconsistent, what
   it should match instead (point at the existing convention/code it should
   follow, the same way the checklist entries below do). If there's nothing
   to report, say so plainly rather than padding out a findings list -
   "reviewed, no issues" is a valid and useful outcome.

## Checklist

### Spacing & layout

- Any new table (`TableBuilder`) uses the same `8.0` left-margin inset every
  other table in the app uses (shrink the rect before building the table,
  not padding inside header cells) - see
  `egui-patterns-reference/references/column-auto-fit-and-reorder.md` for
  the exact pattern and where it's already applied.
- Header rows add real top padding (not just relying on default row height)
  so header text isn't flush against the row's top edge - match the
  `*_HEADER_TOP_PADDING` constant pattern already used (e.g.
  `HEADER_TOP_PADDING` in `itemviewer_helper.rs`, `TAG_HEADER_TOP_PADDING`
  in `tags.rs`), don't invent a third differently-named/valued one for a
  new table without a reason.
- The last row/item in any scrollable list isn't flush against the pane's
  bottom border - reserve real bottom padding as unused rect space (see
  `BOTTOM_PADDING` in `itemviewer.rs`), not an extra fake row or an
  after-the-fact CSS-style margin hack, since `TableBuilder`/`ScrollArea`'s
  own height bookkeeping doesn't leave room for anything virtual on its own.

### Color & theme

- No `egui::Color32::from_rgb(...)` (or similar hardcoded color) for
  anything that should track the app's theme - pull it from `palette:
  &ThemePalette` instead (`src/gui/theme.rs` has the full field list:
  `primary`/`primary_hover`/`primary_active`, `text_normal`/
  `text_header_section`/`tooltip_text_color`, `row_bg`/`row_selected_bg`,
  `icon_color`/`icon_colored_hover`, etc.). A hardcoded color is the single
  most common way a change looks fine in whichever theme you happened to
  be testing in and wrong in the other one.
- If you *did* need a genuinely new color that isn't one of the existing
  semantic fields (rare - check the existing list first), it needs a value
  in **both** the light and dark palette definitions, not just whichever
  theme you were looking at while building the feature.
- Error/destructive states use a consistent red rather than each call site
  picking its own shade (grep for existing `Color32::from_rgb(220, 90, 90)`-
  style usages in preview/error rendering before introducing a new one).

### Icons

- Icons come from `egui_phosphor::regular::*` (already the app-wide set) -
  don't introduce a different icon source/style for one feature. Check
  whether the concept you need an icon for already has one in use elsewhere
  (delete/trash, rename, copy, properties, etc. all have established icons
  already) before picking a new glyph for the same concept.

### Interaction parity

- Destructive actions (delete, permanently delete, remove a group) get a
  confirmation matching the native pattern already used elsewhere - native
  `IFileOperation`/`FOF_WANTNUKEWARNING` dialogs for file operations (don't
  hand-roll a confirmation for something that already gets Explorer's own
  native one for free), or the app's own modal-dialog pattern (see
  `draw_delete_confirmation_popup` in `tags.rs`) for app-level state that
  has no native Explorer equivalent.
- A new right-click menu follows the existing `Popup::context_menu(&resp)`
  + `.close_behavior(PopupCloseBehavior::CloseOnClickOutside)` pattern, and
  reuses `menu_item_button`/`menu_item_button_enabled` (or the tags.rs
  equivalents) rather than hand-building `ui.button(...)` calls with
  different padding/hover behavior than every other menu in the app.
  Destructive entries (delete, permanently delete) are visually
  distinguished the same way existing ones already are.
- A new keyboard shortcut doesn't silently collide with an existing one,
  and if it's meant to mirror a native Explorer shortcut, double check it
  actually fires reliably in this app rather than assuming egui's modifier
  detection matches Explorer's - Shift+Delete's modifier has been observed
  not reaching the handler reliably in this codebase before (see
  `CLAUDE.md`'s Gotchas section) - if a new shortcut depends on a held
  modifier being read correctly, verify it live via
  `eden-explorer-ui-test`, don't assume it from reading the code alone.
- Column tables get the standard "size column to fit / size all to fit /
  move left / move right / move to start / move to end" header menu unless
  there's a specific reason not to (see
  `egui-patterns-reference/references/column-auto-fit-and-reorder.md`) -
  a new table that's missing this while every other table has it is an
  inconsistency a user will notice.

### Text & i18n

- Every user-facing string goes through `i18n.tr("key")` - no bare string
  literals in a `Label`/`Button`/tooltip. Run the `i18n-sync-check` skill
  after adding any new key to confirm all six locales stay in sync (a
  missing translation fails completely silently, with nothing in a normal
  review to catch it otherwise).
- Reuse an existing key when the English text is already identical or
  close enough, rather than adding a near-duplicate (`i18n-sync-check`'s
  "possibly unused" output is also a decent signal for near-duplicates that
  crept in over time).
