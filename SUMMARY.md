# EdenExplorer — Project Summary

## What it is
EdenExplorer is an open-source, high-performance file manager for Windows 10/11, built in Rust with [egui](https://github.com/emilk/egui/) (immediate-mode GUI). It's positioned as a fast, modern, FOSS alternative to Windows File Explorer, using direct NT-level filesystem APIs for performance instead of relying on higher-level Windows shell abstractions.

- **License:** MIT
- **Current version:** 0.28.0 (`Cargo.toml`) — bumped automatically by the release pipeline
- **Status:** Public alpha, actively developed
- **Repo owner:** mtucciarone/EdenExplorer

## Core technology
- **Language:** Rust (edition 2024)
- **GUI:** egui / eframe 0.35, egui_extras, egui-phosphor (icon set)
- **Windows integration:** `windows` crate (0.62) with extensive Win32 feature set (Shell, filesystem, registry, DWM/GDI, portable devices, DirectWrite, globalization, etc.), plus `ntapi` for direct NT-level filesystem access
- **Concurrency:** `rayon` (parallelism), `crossbeam-channel` (async/background communication), `num_cpus`
- **Persistence:** `bincode` / `postcard` (binary settings cache), `serde` / `serde_json`
- **i18n:** `fluent` + `fluent-bundle` + `unic-langid`, with embedded locale assets via `rust-embed` and OS locale detection via `sys-locale`
- **Other:** `image` (thumbnails/icons), `rfd` (native dialogs), `lru` (caching), `chrono`, `dirs`, `rand`

## Project layout (`src/`)
- **`main.rs` / `build.rs`** — entry point and Windows resource/icon build script (`winres`)
- **`core/`** — non-UI logic:
  - `drives.rs` — drive enumeration/detection (including raw/unmounted drives)
  - `fs.rs` — filesystem operations, tag/search sentinel-path encoding
  - `sqlite.rs` — read-only SQLite access (tables/views, paged rows) behind a path model like `archive_view`'s
  - `indexer.rs` — settings/tags/favorites/tab-groups/custom-themes persistence
  - `launch.rs` — launching files/programs, `shell:`/CLSID resolution
  - `portable.rs` — portable device support (iPhone, Android, etc.)
  - `network.rs` / `network_mdns.rs` / `network_wsd.rs` — SMB share browsing, mDNS and WS-Discovery device discovery
  - `everything.rs` — voidtools Everything + built-in filesystem search engines
  - `preview.rs` / `video.rs` / `audio.rs` / `mermaid.rs` / `syntax_highlight.rs` — preview-pane content rendering
  - `checksum.rs` / `compress.rs` / `robocopy.rs` — checksums, zip compression, paste/move copy engine
  - `context_menu_settings.rs` / `tab_groups.rs` / `send_to.rs` — user-defined custom context-menu commands, saved tab groups, and Send To destination groups
  - `utils/` — clipboard, colors, dialogs, file helpers, fonts, sorting, tabs, text, thumbnails, widgets
- **`gui/`** — UI layer:
  - `mod.rs`, `theme.rs`, `icons.rs`, `i18n.rs`, `utils.rs`, `dragdrop.rs`
  - `windows/` — application windows/panels: main window (+ impl split), navigation, settings (including the Network page where network locations are added/edited), about, theme customization, drag-and-drop, shell context menu, Windows-specific overrides
  - `windows/containers/` — explorer view, item viewer (+ gallery/navbar/helper variants), sidebar, tabs, tags, topbar, shared structs/enums
- **`locales/`** — translation files (English, Indonesian, Japanese, multiple Chinese variants)
- **`scripts/`, `logo/`** — build/release helper scripts and branding assets
- **`.releaserc.json`** — semantic-release configuration for automated releases
- **`.claude/skills/`** — project skills capturing reusable project knowledge:
  `eden-explorer-ui-test` (build/launch/drive the app for live verification),
  `egui-patterns-reference` (documented egui techniques used here),
  `egui-design-review` (UI/UX consistency checklist),
  `i18n-sync-check` (locale-parity checker script)

## Project documentation
- **`README.md`** — public-facing overview, feature list, roadmap, build/debug instructions, keyboard shortcuts
- **`FEATURES.md`** — day-by-day log of recently added features and fixes (rolling, most recent days only - the full list always lives in `README.md`)
- **`PRD.md`** — product requirements: positioning, personas, principles, capability inventory, known gaps, planned and proposed enhancements, open questions
- **`CHECKLIST.md`** — phase-by-phase delivery tracking (`Completed` / `In-Progress` / `Pending`)
- **`CLAUDE.md`** — engineering guide: architecture map, conventions, safety rules, recorded mistakes, operational instructions
- **`SUMMARY.md`** — this file
- **`SECURITY.md`** — security policy

## Key features
- Root-level drive view on startup with detailed storage info
- Asynchronous/streaming directory scanning that doesn't block the UI
- Tabbed browsing with pinned tabs, independent loading states, and full navigation history (back/forward/up)
- Real-time fuzzy search/filtering with cached indices
- Sidebar with favorites (drag-and-drop reorderable) and quick access
- Dark/light themes with a full custom color palette editor (hex/RGB/HSL input), layout options (row density, sidebar width), and twelve prebuilt two-color presets (an accent plus a color-theory-contrasted secondary that drives the toolbar icon row, all buttons, and dialog accents); user-created custom themes now save/restore your *entire* edited palette (not just the accent/secondary pair), so reselecting one - even after restarting - brings back every color you'd customized; nearly every UI surface is genuinely palette-driven and editable - sidebar, status bar, address bar, search box, toolbar (including a unified hover color/background across every toolbar icon, view-layout icons included), preview pane, Settings sidebar selected-item text/icon, the navigation toast, and notifications (including semantic green/yellow/red status colors), plus primary-button text and a real disabled-button color pair; a live "Live Preview" panel in Settings → Appearance mocks up every themed surface as you edit, including an interactive row you can hover to see the real hover colors; consistently-styled dialogs (Checksums, Paste Conflict, Bulk Rename) and a themed Tags settings page; clearly visible borders, leading search icons, inline close buttons, and auto-focus on the item-viewer filter box, the navbar search box, and the preview pane's Find bar
- Tagging system for files/folders with tag-colored views, plus custom folder colors
- Gallery/thumbnail view (small to extra-large) for media
- Context menu operations (cut/copy/paste/rename/delete), optional native Windows shell context menu integration
- Drag-and-drop both within the app and to/from native Windows (Explorer, Desktop, other apps)
- Elasticsearch/OpenSearch browsed like folders: indices are folders, documents are JSON files, scroll-paged, read-only
- MongoDB browsed like folders: database, collection, document - hand-rolled OP_MSG/BSON with SCRAM-SHA-256, read-only
- CouchDB browsed like folders: databases are folders, documents are JSON files, read-only
- SQLite databases browsed like folders: tables and views with row counts, paged row grids, strictly read-only
- Spring-loaded folders: resting on a folder mid-drag opens it, and releasing anywhere in what opened drops there - so one gesture can descend several levels, including into a folder that is empty
- Paste/drag-and-drop name-collision handling: a themed Replace / Skip Existing / Rename dialog (both flows share the same conflict-check + robocopy pipeline), with safe auto-numbered renaming that never overwrites the original file mid-operation
- Compress to zip: a context-menu entry that archives the selection in the background (named after the single item, or the shared parent folder for a multi-selection; never overwrites an existing zip of the same name)
- Recent locations: a sidebar section tracking the most-recently-visited real folders (capped at 20, excludes anything already in Favorites), recorded from the single choke point every navigation already passes through to load a directory
- Global search (search icon / Ctrl+F in the navigation bar) with a recursive "This folder" / "Everywhere" scope toggle, powered by voidtools Everything (with automatic fallback to a built-in filesystem search when Everything isn't installed/running, also user-selectable in Settings) - results open as a normal tab compatible with every view mode and show each match's source folder; both engines support `*`/`?` wildcard queries, plain keyword matching, and `key:value` filter syntax (`ext:pdf`, `size:>10mb`, `modified:today`, etc.), discoverable via an info icon next to the search box
- Saved Searches: a dedicated sidebar section (separate from Favorites/Tags) for one-click-saved queries + scope, reopened via the same search-tab mechanism a live search uses; capped at 50, renamable/deletable via right-click
- Content search: `content:<word>` filter (folder-scoped only) alongside `ext:`/`size:`/`modified:` - a streamed, text-only line scan for the built-in engine, or Everything's own native `content:` keyword (requires Content Indexing enabled in Everything itself)
- Shared Network device discovery via legacy SMB/NetBIOS browsing, an active subnet sweep, and (new) hand-rolled mDNS + WS-Discovery queries - each candidate device is still verified over SMB before it's shown, so every entry is guaranteed browsable
- Portable device support and raw/unmounted drive (ISO, Linux partition) detection
- Native Recycle Bin support
- Multi-language UI (English, Indonesian, Japanese, Chinese variants) via Fluent i18n
- Persistent settings via an efficient binary cache; column sort/view state persists per directory
- Rich keyboard shortcut set (tab management, navigation, rename, properties, refresh, etc.)
- Six view modes (Details, Detail+Preview, Gallery, Columns, Column+Preview, Preview) with per-directory column layout, fit-to-content, and reordering
- Preview pane for text (with syntax highlighting for code files), Markdown (rendered, including highlighted fenced code blocks and native-rendered Mermaid flowchart/sequence diagrams), images, animated GIFs, SVG (rasterized, including gzip-compressed .svgz), video, audio (waveform + transport controls), PDF/Word/EPUB text extraction, and zip/7z archive listings — with selectable text and right-click copy. Video and audio playback never starts or loops on its own - it waits for the user to press play, and stops at the end
- Delete to Recycle Bin plus an explicit "Delete Permanently" action using the native Windows confirmation
- Explorer-style click-pause-click inline rename (extension excluded from the selection)
- Bulk rename: multi-selection "Rename" opens a live-preview dialog (simple find/replace or regex with capture groups, plus `{name}`/`{ext}`/`{n}`/`{n:03}` placeholders), with cross-item and existing-file collision detection blocking the commit until resolved, applied as one batched native operation
- `shell:` URI / CLSID resolution in the address bar: `shell:ControlPanelFolder`, `shell:Desktop`, `::{GUID}`, etc. resolve through the real Windows shell namespace - filesystem-backed locations navigate in-app, genuinely virtual ones open externally via the shell
- Undo/Redo (Ctrl+Z/Ctrl+Y/Ctrl+Shift+Z) for Rename, Bulk Rename, Move, and Copy - in-memory only, capped at 50 entries. Replace-resolved conflicts are non-destructive (the overwritten item is recycled, not destroyed) and fully undoable/redoable, re-recycling fresh on each redo so a later undo still has something valid to restore
- Checksums: a single-file context-menu entry computing CRC32/MD5/SHA-1/SHA-256 in one background-threaded pass, with per-hash copy buttons and a paste-to-verify Compare field that checks a pasted checksum against all four algorithms at once
- Multi-tagging: an item can belong to more than one tag group at once (the picker is a multi-select toggle list, staying open across picks); a Tags column shows every tag on a row as a colored chip (own smaller font, truncated with an ellipsis if a name doesn't fit) with "+N" overflow; removing a tag from within one specific tag's own view removes it from that tag only, not every tag the item has
- "Open File Location" context menu entry in the Tag view and Search results (both show items from many different real folders) - opens the item's containing folder in a new tab
- Custom Context Menu: export/import just your custom right-click commands as a standalone file (a full settings export already includes them too), and an unquoted-path placeholder (`%L`, alongside the existing always-quoted `%1`/`%V`) for commands that want to add their own quoting or none at all
- Tab Groups: save the currently-open tabs (or any hand-picked set of folders) as a named group in Settings, then reopen the whole set at once - added alongside current tabs or replacing them - from the "+" new-tab button's menu; the same folder can appear more than once in a group on purpose; an entry can also pair two folders as a dual-pane split (captured automatically from an already-split tab, or added by hand in Settings), so reopening it restores both panes side by side
- Behavior settings: double-click-empty-space-to-navigate-up, an optional per-row selection-checkbox column, middle-click-to-open-in-new-tab, whether last session's tabs restore on launch, defaults for new tabs' view layout and search scope, and independent toggles for whether starting a file operation auto-opens the notification panel and whether toast popups show at all (the notification bell itself always stays visible)
- Multi-row tab strip: once tabs no longer fit at a readable width on one row, the strip automatically wraps onto additional rows (shrinking tab width toward a minimum first); the topbar grows to fit however many rows are currently needed
- Configurable tab gap: an Appearance → Layout setting (0-24px, default 8) controlling the horizontal spacing between tabs, which also feeds into the wrapping math above

## Roadmap (from README)
Upcoming alpha features: multi-tag AND/OR filtering in the tag view. Detailed status lives in `CHECKLIST.md`; rationale and backlog ideas in `PRD.md`.

## Distribution
Distributed as a standalone Windows executable (no install), published as a release on the
self-hosted Forgejo instance that owns this repository.

Releases are fully automated by Forgejo Actions (`.forgejo/workflows/`): pushing to `main`
works out the next version from the commits since the last `v*` tag (`Add …` = minor,
`Fix …`/other = patch, a `BREAKING CHANGE:` footer = major, docs/chore/refactor = no
release), bumps `Cargo.toml` and `Cargo.lock`, runs the tests, builds, tags the bumped
commit, and publishes three assets:

- `EdenExplorer.exe` — the program, downloaded in place by the in-app update check
- `SHA256SUMS.txt` — verified before anything is replaced (a release without it must not be installed)
- `EdenExplorer-<version>-windows-x64.zip` — program plus docs, for updating by hand

The app checks for its own updates against that same forge: `core::release` decides whether a
published release is newer than the running copy, `core::update` fetches it, verifies it against
`SHA256SUMS.txt` and installs it in place, and `gui::windows::update_ui` provides the status-bar
indicator and the offer/download/install modal. The running version is shown at the far left of
the status bar in every view, and About has "Check for Updates" plus a "check automatically"
toggle (at most one automatic check a day, persisted in its own `update_state.bin`).

Supporting scripts live in `scripts/`: `next-release.ps1` (decide the version and write the
release notes), `update-version.ps1` (bump `Cargo.toml` + `Cargo.lock` together), and
`build-and-package.ps1` (build and assemble the assets above). See **Releases & CI/CD** in
`README.md` for the full rules.
