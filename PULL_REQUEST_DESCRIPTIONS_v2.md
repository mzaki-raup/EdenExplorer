## 🆕 What's New

### Tabs & window
- **Multi-row tab strip** — when there are too many tabs to fit at a comfortable width on one row, the strip shrinks tab width down to a minimum first, then wraps onto additional rows below instead of squeezing tabs unreadably thin or scrolling off-screen; the window's own topbar grows to fit however many rows are currently needed.

---
- **Tab close menu** — right-click a tab's close button (or the tab itself) for Close Others / Close Tabs to the Right / Close Tabs to the Left.

---
- **Window position memory** — the app reopens at the same screen position it was closed at, not just the same size.

---
- **Middle-click a breadcrumb segment** to open that folder in a new tab instead of navigating the current one.

---

### Organization
- **Multi-tagging** — an item can belong to more than one tag group at once, via a multi-select toggle picker that stays open across picks.

---
- **Tags column** — every tag on an item shows as its own colored chip, with a "+N" overflow indicator when there are more than fit.

---
- **Per-tag-view tag removal** — remove an item from just the tag group you're currently browsing, without affecting its other tags.

---
- **Recent locations** — a sidebar section tracking the most-recently-visited real folders.

---
- **Saved Searches** — a dedicated sidebar section for one-click-saved queries plus their scope (this folder / everywhere).

---
- **Favorites management page** — rename, delete, reorder, and change the icon or target folder of any favorite, all from one settings page.

---

### Search
- **Global search** — a search icon / Ctrl+F in the navigation bar with a "This folder" / "Everywhere" scope toggle.

---
- **Everything integration** — search is powered by voidtools Everything when it's installed and running, with automatic fallback to a built-in filesystem search when it isn't.

---
- **Search operators** — `*`/`?` wildcards and `key:value` filters (`ext:`, `size:`, `modified:`, `content:`) work in both the Everything-backed and built-in search paths.

---
- **Search results as a real tab** — results open as a normal tab that supports every view mode (Details/Columns/Gallery/Preview), not a separate special-cased screen.

---

### Networking
- **"Shared Network" sidebar section** that auto-detects computers sharing files on the local network, via legacy SMB/NetBIOS browsing, an active subnet sweep, mDNS, and WS-Discovery.

---
- **Direct UNC path browsing** — type `\\SERVER` or `\\SERVER\Share` into the address bar or sidebar to browse it directly, without needing auto-discovery to have found it first.

---

### File operations
- **Undo/Redo** (Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z) for Rename, Bulk Rename, Move, and Copy.
- **Non-destructive Replace-conflict handling for Undo/Redo** — an item overwritten by a Replace-resolved conflict is recycled rather than destroyed, and stays undoable/redoable itself.
- **Bulk Rename** — multi-selection "Rename" (or F2) opens a live-preview dialog instead of renaming one item at a time.

---
- **Bulk Rename find/replace or regex** — plain find/replace, or a full regex mode with capture-group support.

---
- **Bulk Rename placeholders** — `{name}`, `{ext}`, `{n}`, and zero-padded `{n:03}`-style counters.

---
- **Bulk Rename collision detection** — a collision between two of the new names (or with an existing file) blocks committing until it's resolved, shown live in the preview.

---
- **Bulk Rename applies as one batched native operation**, not one rename call per item.

---
- **Paste/drag-and-drop conflict resolution dialog** — a themed Replace / Skip Existing / Rename choice instead of Explorer's own raw prompt.

---
- **Safe conflict-resolution renaming** — the "Rename" choice auto-numbers the new copy without ever overwriting the original mid-operation.

---
- **Checksums** — a context-menu entry computing CRC32, MD5, SHA-1, and SHA-256 in one background pass.

---
- **Per-hash copy buttons**, plus a paste-to-verify Compare field that flags a match/mismatch against a pasted hash.

---
- **Compress to zip** — a context-menu entry that archives the current selection in the background.

---
- **Empty-folder background context menu** — right-clicking empty space in an empty folder now gets the same New Folder/New File/Paste context menu a non-empty folder's background already had, with the newly-created item scrolled into view and selected.

---
- **Click-to-rename gesture** — click once to select, pause, click again to rename in place (not a double-click).

---
- **"Delete Permanently"** context-menu entry, bypassing the Recycle Bin.

---

### Preview pane
- **Rendered Markdown preview**, including native Mermaid diagram rendering.

---
- **Syntax-highlighted code preview.**

---
- **SVG preview**, including gzip-compressed `.svgz`.

---
- **EPUB text extraction** in the preview pane.

---
- **7z archive listing** in the preview pane (alongside the existing zip listing).

---
- **Audio playback** with a waveform display and transport controls.

---
- **Selectable preview text with right-click copy.**

---

### Custom Context Menu
- **Export/Import** — back up or share just your custom right-click commands as their own standalone file, independent of a full settings export.

---
- **`%L` unquoted-path placeholder** — alongside the existing always-quoted `%1`/`%V`, `%L` substitutes the raw path with no quotes, for commands (`clip`, custom scripts, etc.) that want their own quoting or none at all.

---
- **Custom icons from image files**, plus a searchable Phosphor icon picker (1,500+ glyphs), for both custom context-menu commands and favorites.

---

### Appearance — overhaul
- **Nearly every UI surface is now genuinely palette-driven and editable** from Settings → Appearance: sidebar, status bar, address bar, search box, toolbar, preview pane, and notifications.

---
- **Real semantic notification/status colors** — green for success, yellow for in-progress/paused, red for failed/cancelled — instead of one shared color for every state.

---
- **Primary-button text color** is now its own editable field instead of a hardcoded white.

---
- **A genuine disabled-button color pair** (background + text), instead of disabled buttons just looking like a faded enabled one.

---
- **Twelve prebuilt two-color (Prebuilt Themes) presets**, re-picked using real color theory (complementary/temperature contrast, value contrast, a consistent 60-30-10 weighting between the two colors).

---
- **19 single-accent "Color Scheme" quick-swatches** for a one-click primary-color-only retint, expanded from the original 11 with Indigo, Lime, Brown, Amber, Slate, Coral, Mint, and Lavender.

---
- **Full hex/RGB/HSL color picker** for every individual color field.

---
- **Row Density option** (Compact/Comfortable/Spacious) for the file list.

---
- **Sidebar width, Tab Gap, Minimum Tab Width, and Tab Corner Radius** are all now user-configurable, each reflected live in the Appearance page's own preview.

---
- **Settings → Appearance two-column layout** — the scrollable list of settings on the left, a static live preview of Sidebar/Toolbar/Buttons/Notifications/Tabs on the right, so a change is visible without scrolling to find a live example.

---
- **User-created custom themes** — save your current Primary + Secondary colors (and the rest of the palette) as a named theme.

---
- **Custom theme swatch grid** — click a saved custom theme's swatch to apply it instantly; a trash icon on each entry to delete it (with a confirmation naming the theme).

---
- **Editing a saved custom theme** — clicking its swatch pre-fills its name and switches the Save button to "Update Theme", so a tweak-then-save updates that same entry instead of creating a duplicate.

---
- **The last-selected custom theme is remembered across a restart** — its name is already pre-filled (with "Update Theme" showing) the next time you open the app, so you can pick up editing it immediately.
- **Swap Colors button** — swaps the currently-edited Primary and Secondary colors in one click instead of re-picking both by hand to try the opposite pairing.

---
- **Export Theme now bundles every saved Custom Theme**, not just the active palette — importing that file elsewhere restores your whole Custom Themes list too.

---
- **Import Theme merges custom themes by name** — a theme whose name already exists is updated in place with the imported colors; a new name is added as a new entry.

---
- **Circular count badges** on Custom Context Menu/Tab Groups' child-item counts, and the notification bell's unread count, with their own themeable Badge Color.

---

### Settings
- **Export/Import Settings** — back up or transfer general settings, favorites, tags, and both theme palettes (light + dark) as a single JSON file.

---

## 🔄 What's Changed

- Prebuilt themes' secondary color is now a real, significant part of each theme's identity — it drives toolbar icon tinting, the pinned-tab accent, the notification/toast border color, and the Search Active Icon Background, instead of touching almost nothing.

---
- Primary color now also drives Checkbox Hover/Active, Badge Color, and Settings Nav's selected-item text and icon (lightened for readability against the selection background), in addition to the core accent colors it already drove.
- The Notification Border and Toast Border colors are noticeably more visible now (raised alpha), for every prebuilt and custom theme alike.

---
- Buttons across the app — Settings, dialogs, and confirmation popups, including destructive-action buttons — now consistently pick up the theme's button background/stroke/text colors instead of a mix of themed and raw `ui.button(...)` calls.

---
- Small toolbar separators now group the search / view-mode / terminal icon clusters.

---
- Icon buttons throughout got real click-target padding and a hover background.

---
- Sidebar tag-group icons are now genuinely filled (not just outline) and smaller.

---
- Dual-pane active-indicator top line made thicker for better visibility.

---
- Preview pane's filename now sits in its own bordered, bold header instead of a plain line of text.

---
- Settings → Appearance: added top/bottom padding around the Export/Import/Reset Theme buttons.
- Settings → Appearance: the Custom Themes swatch grid now has real gaps between entries (and between wrapped rows), so each entry's own delete button is unambiguous.
- Settings → Appearance: a Custom Theme's saved name now shows in full and left-aligned under its swatch, with the card auto-widening to fit a long name, instead of being truncated and centered.

---
- Settings → Appearance: Primary, Swap Colors, and Secondary are laid out as a compact 2-column block (Swap Colors sitting directly between the two color rows) instead of spanning the full settings-panel width.

---
- Settings → Tags: added a page title (matching every other settings page) and increased the spacing between tag-group cards.
- Accent-driven color scheme now retints the sidebar, explorer, and file list backgrounds, inactive/active tabs, toolbar icons, and borders together, not just buttons.

---
- Standardized settings form styling (field padding, background/text contrast, readable selected-text color) across every Settings page.

---
- Icon picker is now a popup opened from a button, instead of a permanently-expanded grid taking up space on the page.

---
- The Tab Groups tab-strip context menu no longer shows a stray separator above the "Tab Groups" list when there's nothing above it.

---

## 🐛 What's Fixed

- **Appearance editor didn't live-preview**: clicking "Light Mode" while running in Dark Mode silently edited the light palette with zero visible change, since the mode toggle never switched the actual live theme.

---
- **Dead "Resize Handle" color picker**: it edited a field nothing ever read; the real sidebar-resize-handle color came from a different, oddly-named field.

---
- **No app-wide separator color**: every `ui.separator()` line ignored the theme entirely.
- **A saved custom theme could lose its own colors**: reselecting it after changing other settings could revert it to re-derived defaults instead of the exact palette it was saved with; it now stores (and restores) its full palette.
- **Custom theme creation only let you edit the Secondary color**; Primary could only be changed via the separate Core Colors section further down the page, and picking a Secondary color there had no live effect until after saving.
- **Address bar had a double border while editing**, and its height changed between browse and search modes in both single- and dual-pane.
- **Address bar / search box bottom border was intermittently or fully invisible** — root-caused to a translucent stroke color losing an edge to anti-aliasing, and a clip rect cutting off a border drawn flush against its own boundary.
- **Type-to-filter box jumped to the bottom of the view** when a search had no results, instead of staying pinned at the top.

---
- **Custom Context Menu clipboard commands got an unwanted trailing space/newline** — documented and worked around with the new `%L` placeholder plus a `Set-Clipboard`-based recipe.
- **Custom-themes list couldn't set a theme's secondary color when saving** and entries didn't align or click reliably.
- **A custom theme's name was only visible on hover**, not shown anywhere at a glance.
- **Tags settings page's first tag group had an invisible top border** (clipped by the page's own scroll area), and every group's border directly touched the next one.
- **Search box scope toggle used to resize the whole pane** in a narrow split view, or with a longer-language translation; it's fixed-width now.
- **The app's outer window border briefly doubled up along the top edge only** — a second, separate native border was being drawn by Windows' own compositor on top of the app's own painted border.
- **The item viewer's own subtle content border overlapped the app's outer window border** on whichever pane was last/only, reading as one slightly-thicker line.
- **The notification panel clipped after about one row** regardless of how many operations were actually running.
- **Themed buttons visibly resized by a couple of pixels on hover**, with no accompanying color change, across every themed button in the app.
- **The paste-conflict dialog visibly froze for a moment after clicking Replace/Rename** — the slow work (Recycle Bin enumeration, folder-size calculation) was blocking the very frame the dialog needed to disappear on.
- **No toast notification appeared when a file operation started** — only on completion; a toast is now shown immediately for Copy/Move/Delete-style operations too.
- **An already-tagged item couldn't be added to a second tag group** without first removing its existing tag — the right-click menu now always offers "Add Tag" alongside "Remove Tag" when applicable.

---
- **Phosphor icons rendered as CJK characters on systems with a replaced "Segoe UI" font** that happened to squat the icon font's private-use codepoints; the bundled icon font is now subset to its own reserved range so it can never collide with normal text again.
- **Minimum Tab Width could compute a tab narrower than the value the user configured**, for a specific combination of setting value and tab count.

---

## 🔗 Related Issues

<!-- Link to any related issues, using format: Closes #123 -->
