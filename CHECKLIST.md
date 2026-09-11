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
| Completed | Window position memory in addition to size |
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
| Completed | Drag and drop within the item viewer — including that the drag ghost/drop-target highlight now actually clears the instant a drop completes (Details, Detail Preview, and Gallery views); it previously stayed on screen indefinitely since `drag_state.active` was never reset after firing the move, only when something unrelated happened to reset it later |
| Completed | Drag and drop onto breadcrumb folders and directory tabs |
| Completed | Drag and drop out to native Windows targets (Desktop, Explorer, apps) |
| Completed | Optional Windows shell context-menu integration (registry entries) |
| Completed | User-defined custom context-menu commands with custom icons |
| Completed | Auto-refresh of the file list after a custom command completes |
| In-Progress | **Shift+Delete keyboard shortcut** — code path exists and is symmetric with the working plain-Delete path, but the Shift modifier has been measured as not reaching the handler at keypress time. "Delete Permanently" in the context menu is the reliable path today. *Next: root-cause the egui/winit modifier propagation, or document it as a platform limitation.* |
| Completed | **Top-right notification bell + operation history panel** — bell button with an in-progress badge, click-to-open panel listing copy/move/delete operations (icon, item count, destination, status, progress bar), a bottom-right completion toast, and Clear/Clear All (works on in-progress rows too - clears the row from view without touching the underlying operation). All status colors follow the user's own accent (`palette.primary`), not fixed literals. |
| Completed | **Robocopy-backed copy/move/paste engine (Phase B)** — clipboard copy and cut-move (Ctrl+C/X + Ctrl+V) now run through `robocopy.exe` (`core::robocopy`) instead of `IFileOperation`: multi-threaded (`/MT:8`), metadata-preserving (`/COPY:DAT`, `/DCOPY:DAT` for folders), with real Pause/Resume/Cancel controls and a live byte-based progress bar in the notification panel. Pause kills the current job's process (robocopy has no true mid-file pause); Resume re-launches against the same remaining-job queue with `/Z` (restartable mode), so an interrupted file resumes rather than restarting. A name-collision prompt (Replace/Skip Existing/Rename/Cancel) fills the gap robocopy itself doesn't cover (no interactive overwrite dialog of its own) - checked before the job even starts, listing every colliding name; Rename computes the next free `<name>-001`, `-002`, ... and stages the copy in a throwaway subdirectory first so the pre-existing colliding file is never touched (see CLAUDE.md - an earlier version copied under the original name directly into the target dir first, silently overwriting the file "Rename" was supposed to preserve). Verified live: a 5.2 GB / 2000-file cross-drive copy (accurate progress, working Pause/Cancel), and repeated conflict-modal runs (Replace/Skip confirmed via file-system checks). Delete still goes through `IFileOperation`/Recycle Bin, per the original architecture decision - robocopy has no recycle-bin equivalent. Drag-and-drop moves and the external-file-drop dialog are unconverted and still use `IFileOperation`. |
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
| Pending | Recursive / whole-drive search — the largest functional gap vs. Explorer |
| Pending | Content search inside text files |
| Pending | Filter syntax — `size:>100mb`, `ext:png`, `modified:today` |
| Pending | Saved searches in the sidebar |

### Workflow
| State | Item |
|---|---|
| Pending | Bulk rename with pattern / regex preview |
| Pending | Workspaces — tab groups extended to capture split state, view mode, and sort |
| Pending | Dual-pane synchronized scrolling and folder compare |
| Pending | Recent / frequently-visited locations in the sidebar |

### File operations
| State | Item |
|---|---|
| Pending | Operation queue with pause / resume and aggregate progress |
| Pending | Undo stack for moves and renames beyond Recycle Bin restore |
| Pending | Richer same-name conflict resolution UI |
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
