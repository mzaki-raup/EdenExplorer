# EdenExplorer — Development Checklist

Phase-by-phase delivery tracking. Companion to `PRD.md` (the *why* and detail
behind each item) and `CLAUDE.md` (engineering conventions).

**States:** `Completed` · `In-Progress` · `Pending`

> A phase is only marked `Completed` after it has been verified working in the
> running app — not merely compiled. Update this file immediately after that
> verification, per the Operational Instructions in `CLAUDE.md`.

---

## Phase 1 — Core file browsing

| State | Item |
|---|---|
| Completed | Drive-root ("This PC") startup view with type, capacity, and usage per drive |
| Completed | "This PC" sorted by drive letter, independent of leftover folder sort keys |
| Completed | Asynchronous / streaming directory scanning (non-blocking UI) |
| Completed | NT-level filesystem access for enumeration |
| Completed | Back / Forward / Up navigation |
| Completed | Full navigation history saved and restored across sessions |
| Completed | Breadcrumb address bar with inline editing |
| Completed | Windows environment-variable `PATH` expansion in the address bar |
| Completed | Show / hide hidden objects toggle |
| Completed | Type-to-filter in the current directory with fuzzy matching |
| Completed | Cached filter indices for performance |
| Completed | Status bar below every view (item count, files, folders, total size, size-aware selection summary) — present in every view including tag views and independently in both panes of a split |
| Completed | **Global search, powered by voidtools Everything or a built-in filesystem walk** — a search icon (and Ctrl+F) in the navigation bar of every pane opens an inline query box with a "This folder" / "Everywhere" scope toggle ("This folder" is recursive - it searches all subfolders); submitting opens a normal tab (its own `::SEARCH::`-sentinel virtual location, same trick as This PC/Recycle Bin/tag views) populated with real `FileItem`s - since it's populated the same way a real directory listing is, every existing display mode (Details/Gallery/Columns/ColumnPreview/Preview/DetailPreview) and split-pane already work with no new rendering code. A "Search engine" setting (Settings → Behavior, alongside the default-scope setting) picks Everything (near-instant, index-backed) or Built-in (always available, walks the real filesystem live); if Everything isn't installed/running, Built-in is used automatically regardless of the setting. Search results show an extra "Original Directory" column (reusing the Recycle Bin's own column, since both need "where did this actually come from") since matches can come from many different folders, unlike a normal listing. Result count capped at 5,000 per query for either engine. Supports `key:value` filter syntax alongside plain text - `ext:pdf`, `size:>10mb`/`size:<1kb`, `modified:today`/`modified:yesterday`/`modified:thisweek` - which Everything's own query parser already accepted natively; the built-in engine gained a matching `parse_search_filters` parser (`core/fs.rs`) so the same query works identically on both engines. An info icon next to the search box surfaces the exact filter syntax so it's discoverable without reading docs. |
| Completed | **Saved Searches** — a bookmark icon next to the search box saves the current query + scope (name defaults to the query text, renamable afterward) into a new dedicated "Saved Searches" sidebar section (its own collapsible group, mirroring Tags' structure - not folded into Favorites, since a saved query/scope isn't "a folder visited often"). Clicking a saved search reopens it as a normal search tab via the same `open_or_focus_search_tab` a live search uses; right-click gives Open in new tab / Rename / Delete. Persisted to its own `saved_searches.bin` (mirroring Tags' persistence pattern), capped at 50 entries (a one-click save accumulates faster than deliberately dragging a folder into Favorites, so an unbounded list would clutter the sidebar) - the save button shows a "limit reached" message instead of silently failing once at the cap. |
| Completed | **Content search** — `content:<word>` filter, in addition to `ext:`/`size:`/`modified:`. For the built-in engine, matches are found by streaming a case-insensitive line scan (`core/fs.rs`'s `matches_content_filter`) over candidate files that already pass every other filter, skipping directories, anything over 8MB, and anything that doesn't look like text (reusing `core::preview`'s existing `is_known_text_extension`/`sniff_is_text` checks) - deliberately restricted to a specific folder scope (not "Everywhere"), since scanning file contents across every drive would be far slower than a normal filename search. For the Everything engine, `content:` is Everything's own native query keyword and needs no code change - but it only returns matches if the user has separately enabled Content Indexing in Everything's own settings (off by default, with no way for this app to detect that via IPC); the search box's info-icon hint calls this out. |

## Phase 2 — Tabs, panes & window management

| State | Item |
|---|---|
| Completed | Tabbed browsing with independent per-tab loading state |
| Completed | Tab pinning with automatic restore on startup |
| Completed | Tab context menu — Close, Close Others, Close to the Right, Close to the Left |
| Completed | Tab groups (named sets of paths opened as a batch) |
| Completed | Split-pane view with per-pane persistence |
| Completed | Collapsible sidebar |
| Completed | Window size modes — fullscreen, half-screen, custom dimensions |
| Completed | Window position memory in addition to size — fixed a physical/logical pixel unit mismatch (`GetClientRect` vs. egui's logical points) in the drag/resize-finished save path that inflated the remembered size on any display scaled above 100%, throwing off both size and position on the next launch |
| Completed | Maximization bounds and minimum-size constraints |
| Completed | CLI accepts `::MY_PC::`, `::RECYCLE_BIN::`, `shell:MyComputerFolder`, `shell:RecycleBinFolder` |

## Phase 3 — View modes & column system

| State | Item |
|---|---|
| Completed | Details view |
| Completed | Detail + Preview view |
| Completed | Gallery view with five thumbnail sizes |
| Completed | Columns view (Finder-style progressive browser) |
| Completed | Column + Preview view |
| Completed | Preview view (list beside a large preview pane) |
| Completed | Column show / hide, drag-resize, and reorder (left / right / start / end) |
| Completed | "Size column to fit" and "Size all columns to fit" |
| Completed | Lexicographic multi-column sorting (Shift+Click add, Ctrl+Click remove) |
| Completed | Per-directory persistence of column layout, sizing, sort, and view mode |
| Completed | Auto-fit columns on first open without overriding a manual resize |
| Completed | Recycle Bin columns (Original Directory, Deleted) included in fit-to-content measurement |
| Completed | Tag-view table column parity — fit, reorder, header context menu |
| Completed | Consistent 8px left inset and header top padding across every table view |
| Completed | Bottom padding below the last row in Details and Gallery views |

## Phase 4 — File operations & shell integration

| State | Item |
|---|---|
| Completed | Cut / Copy / Paste / Rename / Delete / Properties |
| Completed | Copy Path (Ctrl+Shift+C) |
| Completed | Native `IFileOperation`-backed operations with real Explorer progress dialogs |
| Completed | Copy / paste overwrite handling for existing files and folders |
| Completed | Delete to Recycle Bin as the default action |
| Completed | "Delete Permanently" context-menu action with the native warning dialog |
| Completed | Native Recycle Bin browsing, Restore, and Delete Permanently |
| Completed | Inline rename with Explorer-style click-pause-click gesture |
| Completed | Rename selects base filename only, excluding the extension |
| Completed | Rename selection visible on the first frame (no highlight lag) |
| Completed | Rename gesture disabled on drives / "This PC" |
| Completed | Rename support in the Preview view |
| Completed | Live invalid-character filtering while renaming |
| Completed | Drag and drop within the item viewer — the drag ghost/drop-target highlight clears the instant a drop completes (Details, Detail Preview, and Gallery views); the reset now clears `drag_state.start_pos` and `source_items` too, not just `active`, since a leftover `start_pos` re-armed the drag on the next pointer move. A same-name collision on drop now shows the app's own themed Replace/Skip/Rename modal (routed through the same robocopy conflict-check pipeline as clipboard paste) instead of a raw native `IFileOperation` "confirm replace" dialog, and dropping an item onto the empty background of the folder it's already in is a silent no-op instead of surfacing a native "source and destination are the same" error |
| Completed | Drag and drop onto breadcrumb folders and directory tabs |
| Completed | Drag and drop out to native Windows targets (Desktop, Explorer, apps) |
| Completed | Optional Windows shell context-menu integration (registry entries) |
| Completed | User-defined custom context-menu commands with custom icons |
| Completed | Auto-refresh of the file list after a custom command completes |
| In-Progress | **Shift+Delete keyboard shortcut** — code path exists and is symmetric with the working plain-Delete path, but the Shift modifier has been measured as not reaching the handler at keypress time. "Delete Permanently" in the context menu is the reliable path today. *Next: root-cause the egui/winit modifier propagation, or document it as a platform limitation.* |
| Completed | **Top-right notification bell + operation history panel** — bell button with an in-progress badge, click-to-open panel listing copy/move/delete operations (icon, item count, destination, status, progress bar), a bottom-right completion toast, and Clear/Clear All (works on in-progress rows too - clears the row from view without touching the underlying operation). All status colors follow the user's own accent (`palette.primary`), not fixed literals. |
| Completed | **Robocopy-backed copy/move/paste engine (Phase B)** — clipboard copy and cut-move (Ctrl+C/X + Ctrl+V) now run through `robocopy.exe` (`core::robocopy`) instead of `IFileOperation`: multi-threaded (`/MT:8`), metadata-preserving (`/COPY:DAT`, `/DCOPY:DAT` for folders), with real Pause/Resume/Cancel controls and a live byte-based progress bar in the notification panel. Pause kills the current job's process (robocopy has no true mid-file pause); Resume re-launches against the same remaining-job queue with `/Z` (restartable mode), so an interrupted file resumes rather than restarting. A name-collision prompt (Replace/Skip Existing/Rename/Cancel) fills the gap robocopy itself doesn't cover (no interactive overwrite dialog of its own) - checked before the job even starts, listing every colliding name; Rename computes the next free `<name>-001`, `-002`, ... and stages the copy in a throwaway subdirectory first so the pre-existing colliding file is never touched (see CLAUDE.md - an earlier version copied under the original name directly into the target dir first, silently overwriting the file "Rename" was supposed to preserve). Verified live: a 5.2 GB / 2000-file cross-drive copy (accurate progress, working Pause/Cancel), and repeated conflict-modal runs (Replace/Skip confirmed via file-system checks). Delete still goes through `IFileOperation`/Recycle Bin, per the original architecture decision - robocopy has no recycle-bin equivalent. Drag-and-drop moves within the item viewer now also route through this same robocopy + conflict-check pipeline (see the drag-and-drop entry above); the external-file-drop dialog is still unconverted and uses `IFileOperation`. |
| Completed | **Double-click empty item-viewer area navigates up** — the existing "double-click background to go up" behavior (Details/Detail Preview views) only ever registered when the folder had visible items, since its handler lived inside the same code branch as the file table itself; an empty folder skipped that branch entirely and silently ignored the double-click. Fixed for Details and Detail Preview; Gallery/Columns/Preview views don't have the `tabbar_action` plumbing this needs yet and are a follow-up. |

## Phase 5 — Preview system

| State | Item |
|---|---|
| Completed | Plain text preview with encoding sniffing |
| Completed | Text extraction preview for PDF and Word (.docx) |
| Completed | Rendered Markdown — headings, emphasis, lists, code, quotes, rules, task lists, simple tables |
| Completed | Image preview |
| Completed | Animated GIF playback |
| Completed | Video playback with play / pause and seek slider |
| Completed | Zip-format archive listing with nesting and per-entry sizes |
| Completed | Text selection with right-click to copy in the text preview |
| In-Progress | **Markdown preview copy fidelity** — right-click copy works, but the paragraph under the pointer contributes its whole text rather than only the selected portion (egui collapses label selection on any button press). See `PRD.md` §7.2 and the doc comments in `itemviewer_preview.rs`. *Next: decide whether a custom rich-text widget is worth the cost, or accept permanently and document in user-facing docs.* |

## Phase 6 — Organization (favorites, tags, groups)

| State | Item |
|---|---|
| Completed | Favorites with drag-and-drop reordering |
| Completed | Favorites management page — rename, delete, change icon, change target, reorder |
| Completed | Custom favorite / context-menu icons from image files, copied into app data |
| Completed | Searchable Phosphor icon picker (1,500+ glyphs) |
| Completed | Tags with custom colors |
| Completed | Tagged items tinted in the file list by tag color |
| Completed | Tags sidebar section; clicking a tag opens or reuses a dedicated tab |
| Completed | Tag view lists Name / Type / Location / Size / Modified |
| Completed | Reorderable tags — drag in Settings, reflected live in the sidebar |
| Completed | Custom folder colors |
| Pending | Multi-tagging — one item belonging to more than one tag |
| Pending | Multi-tag AND/OR filtering in the tag view (depends on multi-tagging) |

## Phase 7 — Theming & appearance

| State | Item |
|---|---|
| Completed | Dark / light mode toggle |
| Completed | Full color palette editor |
| Completed | Accent-driven retinting across sidebar, explorer, list, tabs, toolbar, borders |
| Completed | Thicker accent-colored borders, including an outer window border |
| Completed | Application font selection from system-installed fonts |
| Completed | Standardized settings form styling across every Settings page |
| Pending | Enhanced theme customization — custom layouts, mix/match UI elements and styles |

## Phase 8 — Networking & devices

| State | Item |
|---|---|
| Completed | SMB network browsing (`\\SERVER`, `\\SERVER\Share`, sidebar Network entry) |
| Completed | Shared Network auto-discovery via NetBIOS browsing |
| Completed | Portable device support (iPhone, Android, MTP) |
| Completed | Raw / unmounted drive detection (ISO sticks, Linux partitions) |
| Pending | Additional network discovery protocols — mDNS / WS-Discovery for devices that don't announce over legacy NetBIOS |

## Phase 9 — Persistence & settings

| State | Item |
|---|---|
| Completed | Per-concern binary settings files under `%LOCALAPPDATA%\ExplorerEden\` |
| Completed | Export / Import Settings as a single JSON (settings, favorites, tags, both palettes) |
| Completed | Folder-size scanning toggle |
| Completed | Drive caching (30s) and metadata-keyed icon caching with background loading |
| Completed | Settings pages — General, Behavior, Startup & Window, Appearance, Favorites, Custom Context Menu, Tab Groups, Tags, Advanced |

## Phase 10 — Internationalization

| State | Item |
|---|---|
| Completed | Fluent-based i18n with OS-locale detection |
| Completed | Locales — en-US, id-ID, ja-JP, zh-CN, zh-HK, zh-TW |
| Completed | All code-referenced keys verified present in all six locales |
| Completed | Dead keys removed (`tooltip_show_file_explorer`, `tooltip_show_tags`) |
| Pending | Wire `i18n-sync-check` into CI so untranslated keys fail the build |

## Phase 11 — Engineering infrastructure & documentation

| State | Item |
|---|---|
| Completed | `CLAUDE.md` — architecture map, conventions, safety rules, recorded mistakes, operational instructions |
| Completed | `PRD.md` — product requirements, gaps, planned and proposed enhancements |
| Completed | `CHECKLIST.md` — this file |
| Completed | `eden-explorer-ui-test` skill — build / launch / click / drag / keys / screenshot with focus and occlusion verification |
| Completed | `egui-patterns-reference` skill — four documented egui patterns |
| Completed | `egui-design-review` skill — UI/UX consistency checklist |
| Completed | `i18n-sync-check` skill — locale-parity checker script |
| Completed | Automated release pipeline (semantic-release + GitHub Actions) |
| In-Progress | **README feature list is behind the code** — missing Delete Permanently, preview copy, tag-table columns, padding standardization, and the This-PC sort fix |
| In-Progress | **README documentation drift** — claims a "built-in performance benchmarking system" in two places; no such code exists in `src/` |
| Pending | Change Log file — none exists in the repo yet; decide whether to add one or rely on semantic-release notes |
| Pending | Scripted UI regression suite built on the `eden-explorer-ui-test` skill |
| Pending | Local-only panic / crash logging with a "copy diagnostics" action |

## Phase 12 — Backlog (not scheduled)

Rationale and detail for each item is in `PRD.md` §9.

### Search
| State | Item |
|---|---|
| Completed | Recursive / whole-drive search — see Phase 1's global search entry (Everything or built-in engine, recursive "This folder" or "Everywhere" scope, `*`/`?` wildcards) |
| Completed | Content search inside text files — see the Content search entry above (`content:<word>`, folder-scoped, built-in engine streams a line scan; Everything engine via its own native keyword) |
| Completed | Filter syntax — `size:>100mb`, `ext:png`, `modified:today` — see the Global search entry above (`parse_search_filters` in `core/fs.rs` for the built-in engine; Everything's own query parser already accepted this natively) |
| Completed | Saved searches in the sidebar — see the Saved Searches entry above (dedicated sidebar section, capped at 50, persisted to `saved_searches.bin`) |

### Workflow
| State | Item |
|---|---|
| Completed | **Bulk rename with pattern / regex preview** — selecting multiple items and choosing "Rename" opens a dialog (`gui::windows::containers::bulk_rename`) instead of the single-item click-to-rename gesture. Simple mode: literal find/replace (case-insensitive toggle) plus `{name}`/`{ext}`/`{n}`/`{n:03}` placeholders (`{name}` resolves to the *original* stem, not the post-replace text - deliberate, documented precedence); Regex mode: `regex` crate find/replace with `$1`/`$2` capture-group references, `(?i)` injected for the same case-insensitive toggle, invalid patterns shown inline without ever partially applying. Live preview recomputes only on pattern change (not every frame), with per-parent-directory collision detection (both within the batch and against untouched existing files, correctly excluding names about to be vacated by another item in the same batch) - "Rename All" is hard-blocked while any row collides, is invalid, or the batch is a no-op. Commits via one `IFileOperation` batching every `RenameItem` call before a single `PerformOperations()` (`rename_paths_native`, mirroring `delete_paths_native`'s shape), with a per-item fallback for anything that didn't land, one batched notification, and tag remapping per successfully-renamed item. Modal built on `egui::Area`/`Frame::popup` (not `Window`), following the paste-conflict modal's established pattern. 13 unit tests cover the pattern engine and collision detection directly. |
| Pending | Workspaces — tab groups extended to capture split state, view mode, and sort |
| Pending | Dual-pane synchronized scrolling and folder compare |
| Pending | Recent / frequently-visited locations in the sidebar |

### File operations
| State | Item |
|---|---|
| Pending | Operation queue with pause / resume and aggregate progress |
| Pending | Undo stack for moves and renames beyond Recycle Bin restore |
| Completed | Richer same-name conflict resolution UI — see Phase 4's paste/drag-and-drop conflict modal entry |
| Pending | Checksum / hash panel |

### Preview
| State | Item |
|---|---|
| Pending | Syntax highlighting for code files (plumbing exists via a `TextEdit` custom `layouter`) |
| Pending | More formats — SVG, EPUB, 7z/rar listing, audio waveform and playback |
| Pending | Find-in-preview for long documents |

### Accessibility & input
| State | Item |
|---|---|
| Pending | AccessKit / screen-reader support (also unlocks better UI automation) |
| Pending | Command palette (Ctrl+Shift+P style) |
| Pending | User-rebindable keyboard shortcuts |

### Extensibility & measurement
| State | Item |
|---|---|
| Pending | Documented plugin / extension manifest (command + icon + match rules + optional preview handler) |
| Pending | Reproducible local benchmark script — cold start, 10k-entry listing, idle memory — recorded per release |
| Pending | Track locale coverage and untranslated-key count as a release gate |
