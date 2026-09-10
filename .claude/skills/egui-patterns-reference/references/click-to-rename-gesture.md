# Click-to-rename (click, pause, click again)

**Where it's implemented:** `ExplorerState::click_to_rename_arm` (struct
field), `handle_row_click`/`poll_click_to_rename`/`CLICK_TO_RENAME_DELAY`
(~line 2029) and `handle_editing_file_name` (~line 1032), all in
`src/gui/windows/containers/itemviewer_helper.rs`.

Matches native Windows Explorer's gesture: click an already-selected item,
pause, click it again (not a double-click) to start renaming it in place.
A genuine double-click should still open the item, not rename it.

## Part 1: telling a "slow second click" apart from a double-click

egui reports **both** `clicked()` and `double_clicked()` as `true` on the
frame a genuine double-click completes - so you can't tell "single click on
an already-selected item" (which should *arm* a rename timer) from "second
half of a double-click" (which should *open* the item) just from a single
frame's click event. The two need to reach different code paths despite
being indistinguishable in the instant they happen.

The fix is a timer, armed by the first click and cancelled by anything that
looks like it's not a lone slow second click:

```rust
pub struct ExplorerState {
    /// Armed when a single click lands on an item that was already the
    /// sole selection. If a genuine double-click follows quickly, the arm
    /// is cancelled and the item opens as usual; otherwise, once
    /// CLICK_TO_RENAME_DELAY elapses with the selection unchanged, rename
    /// mode starts.
    pub click_to_rename_arm: Option<(PathBuf, f64)>,
    // ...
}

pub const CLICK_TO_RENAME_DELAY: f64 = 0.35;
```

`handle_row_click` unconditionally clears the arm at the top (any other
kind of interaction cancels it), then in the "this item was already the
sole selection, no modifiers held" branch: a genuine `double_clicked()`
still opens the item immediately (checked *before* arming); otherwise it
sets `click_to_rename_arm = Some((path, now))` and does nothing else this
frame. A per-frame `poll_click_to_rename` call (from the view's top-level
draw function, so it runs regardless of which display mode - Details/
Columns/Gallery/Preview - is active) checks `now - armed_at >=
CLICK_TO_RENAME_DELAY` and, if the armed path is still the sole selection
and nothing else started a rename in the meantime, fires the actual rename
state.

Don't forget to explicitly exclude drives/"This PC" root items from arming
at all (`is_drive_view` guard) - a slow re-click on an already-selected
drive has no rename semantics and should just do nothing, not silently
enter a broken rename-on-a-drive state.

## Part 2: the rename box's text selection must not visibly lag

When rename mode starts, the base filename (excluding the `.ext`) should
appear *already selected* - matching Explorer. The naive order:

```rust
let edit_response = ui.add(TextEdit::singleline(...).id(edit_id));  // 1. create widget
if should_focus { edit_response.request_focus(); }                  // 2. focus it
// set selection range afterward
```

draws the widget **unfocused** on its first frame (so no selection
highlight is visible at all), and only becomes focused - with the highlight
- one frame later. That one-frame gap reads as a visible "flash of
unselected text" before the highlight catches up, especially at typical
frame rates. This was reported as "still slow" even after the *timer*
delay itself had already been reduced - the actual complaint was this
render-order lag, not the arm delay.

The fix: set both the selection range *and* request focus **before**
`ui.add()` creates the widget for the first time, so it renders
already-focused-and-selected on its very first frame:

```rust
if rename_state.should_focus {
    let select_end = /* char count of the filename without its extension */;
    let mut state = egui::widgets::text_edit::TextEditState::load(ui.ctx(), edit_id)
        .unwrap_or_default();
    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
        egui::text::CCursor::new(0),
        egui::text::CCursor::new(select_end),
    )));
    state.store(ui.ctx(), edit_id);                       // 1. pre-store the selection
    ui.memory_mut(|mem| mem.request_focus(edit_id));       // 2. pre-request focus
}

let edit_response = ui.add(
    TextEdit::singleline(&mut rename_state.new_name).id(edit_id)  // 3. THEN create it
);
```

General principle: whenever you need a freshly-created widget to appear
already in some interactive state (focused, selected, scrolled) on its
*first* frame rather than one frame late, write that state via
`ui.memory_mut(...)` / `*State::store(...)` **before** the `ui.add(...)`
call that creates the widget, not after.
