## 🆕 What's New

Compared against the older public README snapshot (which still listed multi-tagging, enhanced theme customization, and network device support as upcoming Roadmap items), the app has since shipped all three, plus a large amount of work the old snapshot predates entirely:

- **Multi-tagging** — an item can belong to more than one tag group at once; a multi-select toggle picker (stays open across picks), a Tags column showing every tag as a colored chip with "+N" overflow, and per-tag-view-scoped tag removal.
- **Network device discovery** — a "Shared Network" sidebar section that auto-detects computers sharing files on the local network via legacy SMB/NetBIOS browsing, an active subnet sweep, mDNS, and WS-Discovery, plus direct `\\SERVER` / `\\SERVER\Share` browsing from the address bar or sidebar.
- **Full theme customization overhaul** — nearly every UI surface is now genuinely palette-driven and editable from Settings → Appearance: sidebar, status bar, address bar, search box, toolbar, preview pane, and notifications (including real semantic status colors — green/yellow/red), plus primary-button text and a genuine disabled-button color pair. Twelve prebuilt two-color presets (re-picked using real color theory), a full hex/RGB/HSL color picker, row-density and sidebar-width layout options, and user-created custom themes (save/apply/edit/delete your own accent + secondary pair).
- **Undo/Redo** (Ctrl+Z/Ctrl+Y/Ctrl+Shift+Z) for Rename, Bulk Rename, Move, and Copy, with non-destructive Replace-conflict handling (the overwritten item is recycled, not destroyed, and stays undoable/redoable).
- **Checksums** — a context-menu entry computing CRC32/MD5/SHA-1/SHA-256 in one background pass, with per-hash copy buttons and a paste-to-verify Compare field.
- **Bulk Rename** — multi-selection "Rename" opens a live-preview dialog (find/replace or regex with capture groups, plus `{name}`/`{ext}`/`{n}`/`{n:03}` placeholders), with collision detection blocking the commit until resolved, applied as one batched native operation.
- **Global search** — a search icon / Ctrl+F in the navigation bar with a "This folder" / "Everywhere" scope toggle, powered by voidtools Everything (with automatic fallback to a built-in filesystem search), `*`/`?` wildcards, `key:value` filters (`ext:`, `size:`, `modified:`, `content:`), and results that open as a normal, view-mode-compatible tab.
- **Saved Searches** — a dedicated sidebar section for one-click-saved queries + scope.
- **Recent locations** — a sidebar section tracking the most-recently-visited real folders.
- **Paste/drag-and-drop conflict resolution** — a themed Replace / Skip Existing / Rename dialog with safe auto-numbered renaming that never overwrites the original mid-operation.
- **Compress to zip** — a context-menu entry that archives the selection in the background.
- **Export/Import Settings** — back up or transfer general settings, favorites, tags, and both theme palettes as a single JSON file.
- **Custom Context Menu: export/import** — back up or share just your custom right-click commands as a standalone file (a full settings export already includes them too).
- **Custom Context Menu: unquoted-path placeholder (`%L`)** — alongside the existing always-quoted `%1`/`%V`, a new `%L` token substitutes the raw path with no quotes, for commands (`clip`, custom scripts, etc.) that want their own quoting or none at all.
- **Custom favorite/context-menu icons from image files**, plus a searchable Phosphor icon picker (1,500+ glyphs) for both.
- Rich preview pane support: rendered Markdown (with native Mermaid diagrams), syntax-highlighted code, SVG (incl. `.svgz`), EPUB text extraction, 7z listings, and audio playback with waveform + transport controls (alongside existing video/image/PDF/zip preview).
- Tab close menu (Close Others / to the Right / to the Left), window position memory, "Delete Permanently", click-to-rename gesture, selectable preview text with right-click copy, and a Favorites management page (rename/delete/reorder/change icon or target in one place).

## 🔄 What's Changed

- Prebuilt themes' secondary color is now a real, significant part of each theme's identity (toolbar icon tinting, pinned-tab/favorite-star accents) instead of touching almost nothing — all 12 presets re-picked using genuine color theory.
- Buttons across the app — Settings, dialogs, and confirmation popups, including destructive-action buttons — now consistently pick up the theme's button background/stroke/text colors instead of a mix of themed and raw `ui.button(...)` calls.
- Small toolbar separators now group the search / view-mode / terminal icon clusters; icon buttons throughout got real click-target padding and a hover background.
- Sidebar tag-group icons are now genuinely filled (not just outline) and smaller.
- Dual-pane active-indicator top line made thicker for better visibility.
- Preview pane's filename now sits in its own bordered, bold header instead of a plain line of text.
- Settings → Appearance: added top/bottom padding around the Export/Import/Reset Theme buttons.
- Settings → Tags: added a page title (matching every other settings page) and increased the spacing between tag-group cards.
- Accent-driven color scheme now retints the sidebar, explorer, and file list backgrounds, inactive/active tabs, toolbar icons, and borders together, not just buttons.
- Standardized settings form styling (field padding, background/text contrast, readable selected-text color) across every Settings page.

## 🐛 What's Fixed

- **Appearance editor didn't live-preview**: clicking "Light Mode" while running in Dark Mode silently edited the light palette with zero visible change, since the mode toggle never switched the actual live theme.
- **Dead "Resize Handle" color picker**: it edited a field nothing ever read; the real sidebar-resize-handle color came from a different, oddly-named field.
- **No app-wide separator color**: every `ui.separator()` line ignored the theme entirely.
- **Address bar had a double border while editing**, and its height changed between browse and search modes in both single- and dual-pane.
- **Address bar / search box bottom border was intermittently or fully invisible** — root-caused to a translucent stroke color losing an edge to anti-aliasing, and a clip rect cutting off a border drawn flush against its own boundary.
- **Type-to-filter box jumped to the bottom of the view** when a search had no results, instead of staying pinned at the top.
- **Custom Context Menu clipboard commands got an unwanted trailing space/newline** — documented and worked around with the new `%L` placeholder plus a `Set-Clipboard`-based recipe.
- **Custom-themes list**: couldn't set a theme's secondary color when saving, entries didn't align or click reliably, and a theme's name was only visible on hover — now a proper, correctly-spaced, clickable grid with pre-fill-to-edit support.
- **Tags settings page**: the first tag group's top border was invisible (clipped by the page's own scroll area), and every group's border directly touched the next one.
- **Search box scope toggle** used to resize the pane in a narrow split view or with a longer-language translation; now fixed-width.

## 🔗 Related Issues

<!-- Link to any related issues, using format: Closes #123 -->

🤖 Generated with [Claude Code](https://claude.com/claude-code)
