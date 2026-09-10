---
name: egui-patterns-reference
description: Reference for non-obvious egui/eframe techniques already worked out and proven in this codebase - forcing a table to re-read its column widths (auto-fit/reorder), building a read-only-but-selectable text widget with a real selection-range API, correctly timing a right-click-to-copy against egui's own selection-reset behavior, and the click-pause-click rename gesture. Consult this BEFORE implementing anything that sounds like "make this table's columns resizable/reorderable", "let the user select and copy this text", "add a right-click copy/context action", or "add a click-to-rename/inline-edit gesture" - these all look like they should be a quick `ui.add(...)` call and are not; each one has a specific trap that cost real time to work through once already. Re-deriving the same fix from egui's source a second time is pure waste when it's written down here.
---

# egui patterns reference

Each file under `references/` covers one non-obvious egui technique this
codebase actually uses, with the working code's file/line location, *why*
the naive approach doesn't work, and the pattern to follow instead. Read the
relevant one before implementing something that rhymes with it - these
aren't theoretical, each was worked out by reading egui's own source because
the docs don't cover the trap.

| If you're about to... | Read |
|---|---|
| Make a table's columns resizable, "size to fit", or drag-to-reorder | `references/column-auto-fit-and-reorder.md` |
| Let the user select and copy text that isn't plain-editable (a preview pane, rendered Markdown, any "read-only" content) | `references/selectable-readonly-text.md` |
| Add a right-click "copy" or any right-click action that depends on *current selection state* | `references/right-click-copy-timing.md` |
| Add a click-to-rename, inline-edit, or any "click, pause, click again" gesture | `references/click-to-rename-gesture.md` |

## The common thread

egui is immediate-mode: almost nothing about a widget's interactive state
persists unless *you* explicitly read it back via a stable `Id` (`ui.id()`,
`ui.make_persistent_id(...)`, or an explicit `.id(...)` on the widget
builder) and a public `*State::load(ctx, id)` accessor, if the widget
exposes one. `Label` deliberately does **not** expose one for its selection
state (`LabelSelectionState` is entirely private) - that's the root cause
behind two of the four patterns above, and it's why `TextEdit` (which does
expose `TextEditState::load`) shows up as the workaround for things that
sound like they should just need a `Label`.

When you hit a new "this should be simple but egui won't let me read the
state back" wall, that's usually the shape of the problem: find the widget
that *does* expose the state you need via a public loader, even if it's not
the widget that "looks like" the obvious choice.
