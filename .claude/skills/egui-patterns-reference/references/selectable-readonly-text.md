# Read-only, but selectable, text

**Where it's implemented:** `draw_selectable_text_with_copy` in
`src/gui/windows/containers/itemviewer_preview.rs`.

## The problem

The obvious widget for "show some text the user can select and copy, but
not edit" is `egui::Label` with `.selectable(true)` (egui's default for
labels anyway). That gets you mouse-drag selection and `Ctrl+C` for free -
but egui's `LabelSelectionState`, which tracks that selection, is entirely
**private**. There is no public API to ask "what text is currently
selected right now" outside of an actual `Ctrl+C`/`Event::Copy` firing. If
you need to *read* the selection for any reason other than "let the OS
clipboard shortcut do its default thing" (see
`right-click-copy-timing.md` for exactly that situation), a `Label` is a
dead end.

## The fix: a `TextEdit` whose buffer is `&str`, not `String`

`egui::TextEdit` (the normal editable widget) exposes exactly what's
missing: `TextEditState::load(ctx, id)` returns the widget's persisted
state, including `state.cursor.char_range()` - a real, public
`Option<CCursorRange>` you can read any time, from anywhere, not just at
the moment a copy event fires.

To make it *read-only* while keeping full selection/copy interactivity
(not `.interactive(false)`, which disables selection too), back it with a
`&str` instead of a `String`. egui ships a `TextBuffer` impl specifically
for this:

```rust
impl TextBuffer for &str {
    fn is_mutable(&self) -> bool { false }
    fn insert_text(&mut self, _text: &str, _ch_idx: CharIndex) -> usize { 0 }
    fn delete_char_range(&mut self, _ch_range: Range<CharIndex>) {}
    // ...
}
```

Both mutating methods are no-ops, so keystrokes visually do nothing to the
content - it behaves read-only - while the widget is otherwise fully
interactive (selection, scrolling, `Ctrl+C`, mouse drag) exactly like a
normal `TextEdit`:

```rust
let text_edit_id = ui.make_persistent_id("preview_pane_text_edit");
let mut text_ref: &str = text;   // note: &str, not String
ui.add(
    egui::TextEdit::multiline(&mut text_ref)
        .id(text_edit_id)                              // stable, explicit Id
        .font(egui::FontId::monospace(palette.text_size))
        .frame(egui::Frame::NONE)                       // looks like a Label, not a text box
        .desired_width(f32::INFINITY),
);
```

To read the selection later (from anywhere, any frame):

```rust
use egui::TextBuffer; // for `.char_range(...)` below

if let Some(state) = egui::widgets::text_edit::TextEditState::load(ui.ctx(), text_edit_id)
    && let Some(range) = state.cursor.char_range()
{
    let char_range = range.as_sorted_char_range();
    if char_range.start != char_range.end {
        let selected: String = text.char_range(char_range).to_owned();
        // ... do something with `selected`
    }
}
```

Note `TextEditState` and `TextBuffer` live at `egui::widgets::text_edit::*`
- `TextEditState` specifically isn't re-exported at the crate root in this
egui version, so use the full path.

## When this doesn't fit: rich/mixed-formatting text

This trick only works for a single contiguous plain string (one font, one
color throughout, or a custom `.layouter()` if you need syntax-highlighting-
style rendering on top of a *single* underlying string). It does **not**
apply to markdown-style rendering with mixed inline formatting split across
many separate `Label` widgets (bold runs, headings, tables as their own
`TableBuilder`) - there's no single buffer to point a `TextEdit` at. For
that case, see `right-click-copy-timing.md`'s section on the multi-widget
fallback instead; it takes a different, lower-fidelity approach on purpose.
