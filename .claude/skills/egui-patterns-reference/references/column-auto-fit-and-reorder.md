# Column auto-fit, "size to fit", and drag-to-reorder

**Where it's implemented:** `ItemViewerColumnState` in
`src/gui/windows/containers/structs.rs` (~line 327) for the main Details/
Recycle Bin table, `TagColumnState` (~line 1075, same file) for the smaller
per-tag-group table, `compute_item_viewer_column_layout` in
`itemviewer_helper.rs` (~line 2198), table construction in
`itemviewer.rs` (~line 380-420).

## The problem

`egui_extras::TableBuilder` takes `Column::initial(width)` per column, but
that value is only *consulted* the first time a table with a given `Id` is
rendered. After that, egui persists the user's actual column widths (from
manual resize-dragging) in its own memory keyed by that `Id`, and your
`initial()` argument is silently ignored on every subsequent frame -
including frames where you specifically wanted to *force* new widths (the
user clicked "size column to fit", or a header got dragged to a new
position and needs different width bookkeeping).

Naively recomputing "the right width" and passing it as `initial()` again
therefore does nothing once the table has rendered once. There's no public
API to reach into egui's internal per-column-width memory and overwrite it
directly.

## The fix: change the table's `Id` on purpose

Give the table a `layout_generation: u64` counter, and fold it into the
`TableBuilder`'s id salt:

```rust
let table_id_salt = egui::Id::new((
    "item_viewer_table",
    active_tab_id,
    &current_dir,
    is_drive_view,
    is_recycle_bin_view,
    column_state.layout_generation,   // <-- the trick
));
```

Bump `layout_generation` (`wrapping_add(1)`) whenever you want the table to
forget its persisted widths and re-read `initial()` fresh: a "size column to
fit" / "size all columns to fit" click, a move-left/right/to-start/to-end
reorder, or (see below) the very first render of a freshly-opened view.
Since the `Id` is now different, egui treats it as a brand-new table with no
memory of the old one, and your freshly-computed `Column::initial(...)`
values are read again.

The widths themselves are computed by walking the currently-visible rows and
measuring each cell's rendered text width (see
`compute_item_viewer_column_widths` in `itemviewer_helper.rs`) - measure
**every column your table actually shows**, including ones that are easy to
forget. The Recycle Bin's `Original Directory`/`Deleted` columns were
originally left out of that measurement loop and silently stayed at
header-label width forever, because the per-row loop only updated
`name`/`type_width`/`size_width`/`modified_width`/`created_width` - the fix
was adding the missing two lines to the loop, not touching the fit-request
plumbing at all. If you add a new column to a table, make sure it's both
(a) in the `TableBuilder::column(...)` list and (b) in the width-measuring
loop - it's easy to do one and forget the other.

## Auto-fit once, on first open, without fighting a manual resize

Don't force-refit every render (that would undo the user's manual drag every
frame) or hook it to a navigation event (misses split-panes, tabs restored
from a session, etc.). Instead, check at render time whether the current
widths still equal the type's `Default` (meaning nothing's customized them
yet), gated by a one-shot `auto_fit_checked: bool` so it only fires once per
view-open:

```rust
if fit_request.is_none() && !column_state.auto_fit_checked {
    column_state.auto_fit_checked = true;
    let sizes_are_default = /* compare current widths against Default::default() */;
    if sizes_are_default {
        fit_request = Some(FitRequest::All);
    }
}
```

This fires correctly regardless of *how* the view got loaded (first tab at
startup, switching to an already-open tab, a brand new tab, a split pane) -
every one of those is "a render happens", which is the only thing this
check depends on.

## Reordering

`Move left / Move right / Move to start / Move to end` on a column just
mutates an `order: Vec<Column>` (swap-with-neighbor, or remove+reinsert at
an end) and then bumps `layout_generation` the same way a fit request does -
reordering and re-fitting share the exact same "give the table a new Id"
mechanism. See `TagColumnState::move_left/move_right/move_to_start/
move_to_end` for the smaller, self-contained version of this (no
visibility-toggle complexity, easier to read as a template than the full
`ItemViewerColumnState`).

## Left margin, consistent across every table

All of these tables share one `const left_margin = 8.0` inset (shrink the
table's rect before building it, rather than padding inside each header
cell) - see `itemviewer.rs` ~line 383. If you add a new table anywhere in
the app, match this value so column content starts at the same x-position
everywhere; it was specifically called out as a cross-view consistency
requirement once already (the tag-group table didn't have it and needed
retrofitting).
