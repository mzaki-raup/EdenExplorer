# Right-click-to-copy: why timing, not the click handler, is the hard part

**Where it's implemented:** `draw_selectable_text_with_copy` (single-buffer
case) and `arm_markdown_copy_on_right_click` (multi-widget case), both in
`src/gui/windows/containers/itemviewer_preview.rs`.

## The trap

The obvious approach - "check `response.secondary_clicked()`, then read the
selection and copy it" - silently copies the **wrong thing** (usually
nothing, or the whole widget's text instead of just the selection). This
isn't a bug in your click-detection; it's egui's own selection code working
against you:

Inside egui's text-cursor handling (`TextCursorState::pointer_interaction`,
used by both `Label` and `TextEdit`), **any** pointer button press -
including the *right* button - that lands on a selectable text widget
resets its selection to a single point at the click location, *before* that
widget's own click handler runs:

```rust
} else if response.sense.senses_drag() {
    if response.hovered() && ui.input(|i| i.pointer.any_pressed()) {
        // "any_pressed" = ANY button, not just primary.
        // This collapses whatever was selected, right here.
        self.set_char_range(Some(CCursorRange::one(cursor_at_pointer)));
```

So by the time `response.secondary_clicked()` becomes `true` (which fires on
the *release*, one or more frames after the press already ran the code
above), the selection you wanted to copy is already gone. There is no
sequencing trick within that one widget's own render call that avoids this
- the reset and your reaction to the click happen in the same call, reset
first.

## The fix: read state from *last* frame, react on the *press*, not the click

Don't wait for `secondary_clicked()`. Check for the press itself
(`i.pointer.button_pressed(PointerButton::Secondary)`), which is `true` on
the exact frame the button goes down - and read the selection **before**
that frame's widget-render call has a chance to touch it, using state that
was persisted at the *end of the previous frame*:

```rust
let secondary_just_pressed =
    ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary));

if secondary_just_pressed
    && let Some(pointer_pos) = ui.input(|i| i.pointer.interact_pos())
    && panel_rect.contains(pointer_pos)
    && let Some(state) = egui::widgets::text_edit::TextEditState::load(ui.ctx(), text_edit_id)
    && let Some(range) = state.cursor.char_range()
{
    // `state` here is last frame's cursor - the widget below hasn't
    // rendered (and reset it) yet this frame.
    let char_range = range.as_sorted_char_range();
    if char_range.start != char_range.end {
        ui.ctx().copy_text(text.char_range(char_range).to_owned());
    }
}

// THEN render the widget as normal - its own reset-to-click-point still
// happens, which is fine, we already copied what we needed above.
ui.add(egui::TextEdit::multiline(&mut text_ref) /* ... */);
```

This only works because `TextEditState::load` is a public, stateless read -
see `selectable-readonly-text.md` for why that has to be a `TextEdit`, not
a `Label`. `ctx.copy_text(String)` writes straight to the OS clipboard
without going through any widget's internal copy machinery at all.

## The multi-widget fallback (rendered Markdown, many separate `Label`s)

When the selectable content is spread across many `Label` widgets (headings,
paragraphs, bold runs each their own widget - see `draw_markdown`), there's
no single `TextEditState` to snapshot; `LabelSelectionState` (which *does*
track cross-widget selection) is entirely private, so there's no equivalent
read-last-frame trick available.

The fallback used here leans on egui's *own* built-in multi-widget copy
instead of trying to read anything: inject a synthetic `Event::Copy` the
moment a right-press is detected, before any label in this frame has
rendered, and let egui's normal Ctrl+C-equivalent machinery do the rest:

```rust
if secondary_just_pressed && panel_rect.contains(pointer_pos) {
    ui.ctx().input_mut(|i| i.events.push(egui::Event::Copy));
}
// ... then render all the markdown labels as normal
```

**Known, accepted trade-off:** the *specific* label the click physically
landed on still gets its selection reset to empty by the press (same root
cause as above) - and an *empty* range makes `Label`'s own
`selected_text()` fall back to copying that **whole** widget's text, not
just the part that was selected. Labels elsewhere in the selection (not
under the pointer) are unaffected and copy their exact selected text. In
practice this means: right-click near where you finished dragging (the
common case - your cursor naturally ends up there) copies correctly, with
the one paragraph under the pointer possibly over-including a little extra
text from that paragraph. This was verified as an acceptable trade-off
through live testing rather than assumed - if you're touching this code,
don't "fix" the over-inclusion without re-deriving why a real fix isn't
available (see `selectable-readonly-text.md`'s note on why this case can't
use the `TextEdit` trick).
