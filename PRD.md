# EdenExplorer — Product Requirements Document

**Status:** Public alpha · **Version:** 0.27.0 (`Cargo.toml`) · **License:** MIT
**Platform:** Windows 10/11 (x64) · **Repo:** `mtucciarone/EdenExplorer`

> This PRD is reverse-engineered from the current codebase (`src/`) plus the
> README's roadmap, and is meant to be a living document. Sections marked
> **[Proposed]** are ideas raised for discussion — they are *not* commitments
> and haven't been agreed with the maintainer. Sections marked **[Planned]**
> come from the README's own "Upcoming Alpha Features" list.

---

## 1. Product overview

EdenExplorer is a standalone, no-install file manager for Windows, built in
Rust with the egui immediate-mode GUI toolkit. It reaches the filesystem
through direct NT-level APIs and Win32/Shell COM interfaces rather than
higher-level abstractions, and ships as a single executable with no
telemetry and no background services.

**One-line positioning:** a fast, modern, free alternative to Windows File
Explorer for people who spend real time in a file manager.

## 2. Problem statement

Windows File Explorer is the default for hundreds of millions of users but
has stagnated: slow directory enumeration on large folders, a UI shaped by
constraints from a different era, limited organizational tools (no real
tagging, no persistent workspaces), and increasing amounts of
non-user-serving surface area. The main third-party alternatives split into
paid closed-source tools (FilePilot) or slower managed-runtime tools
(Files Community).

EdenExplorer's bet is that a native-speed, open-source file manager with
modern organizational primitives can win the users who feel that friction
daily — developers, power users, and anyone managing large media or project
directories.

## 3. Target users

| Persona | Needs | What the product does for them |
|---|---|---|
| **Power user / developer** | Fast navigation, keyboard control, many parallel contexts | Tabs + tab groups, split view, rich keyboard shortcuts, breadcrumb path entry with env-var expansion, terminal launch |
| **Media / creative worker** | Visual browsing of large image and video folders | Gallery view with sizes from small→extra-large, image/GIF/video preview, column+preview layouts |
| **Organizer / knowledge worker** | Structure beyond folder hierarchy | Tags with colors and a dedicated per-tag view, favorites with custom icons, per-directory persisted sort/column layout |
| **Privacy-conscious user** | No telemetry, no account, no install | Single portable executable, local-only binary settings, MIT-licensed and auditable |
| **Multilingual user** | Native-language UI | Fluent-based i18n in 6 locales, OS-locale auto-detection |

## 4. Product principles

1. **Speed is the feature.** NT-level enumeration, async/streaming directory
   scans, cached drive and icon metadata. The UI never blocks on I/O.
2. **Native behavior where users expect it.** File operations go through
   `IFileOperation`, which means real Explorer progress dialogs, real
   confirmation prompts, real Recycle Bin semantics — not reimplementations.
3. **No telemetry, no lock-in.** All state is local, in a documented folder,
   exportable and importable as one JSON file.
4. **Portable by default.** One `.exe`, no installer, no runtime dependency.
5. **Every visible string is translatable.** No hardcoded UI text.

## 5. Current capabilities (implemented)

### 5.1 Navigation & window management
- Drive-root ("This PC") startup view listing all drives with type, capacity,
  and usage, sorted by drive letter.
- Back / Forward / Up with full history persisted and restored across
  sessions.
- Breadcrumb address bar with click-to-edit, Windows environment-variable
  `PATH` expansion, and drag-and-drop onto breadcrumb segments.
- Tabbed browsing with independent per-tab loading state, pinned tabs
  (auto-restored at startup), and a tab context menu (close, close others,
  close to the right/left).
- **Tab groups** — named sets of paths that open as a batch of tabs.
- Split-pane view (two independent panes side by side).
- Collapsible sidebar; window position *and* size restored on relaunch;
  fullscreen/half-screen/custom window-size modes.

### 5.2 View modes (six)
`ItemViewerDisplayMode` in `containers/structs.rs`:
- **Details** — the classic sortable table.
- **DetailPreview** — Details table plus a preview pane.
- **Gallery** — thumbnail grid, five thumbnail sizes.
- **Columns** — macOS Finder-style progressive column browser.
- **ColumnPreview** — column browser plus preview pane.
- **Preview** — file list beside a large preview pane.

Table columns support show/hide, drag-resize, reorder (move left/right/to
start/to end), "size column to fit" / "size all columns to fit", and
lexicographic multi-column sorting (Shift+Click adds, Ctrl+Click removes).
Column layout, sizing, and view mode persist **per directory** across
sessions.

### 5.3 File operations
- Cut / copy / paste / rename / delete / properties, via context menu and
  keyboard.
- Delete → Recycle Bin by default; **Delete Permanently** as an explicit
  context-menu action (with the native "permanently delete?" warning).
- Native Recycle Bin browsing with Restore and Delete Permanently, showing
  Original Directory and Deleted timestamp columns.
- Inline rename with Explorer-style click-pause-click gesture; extension
  excluded from the initial selection; live invalid-character filtering.
- Drag and drop: within the app, onto breadcrumbs/tabs, and out to native
  Windows targets (Desktop, Explorer, other apps).
- Optional Windows shell context-menu integration (registry-provided
  entries appear inline).
- **Custom context-menu commands** — user-defined commands with custom
  icons, invoked against the selection or the folder background, with
  automatic file-list refresh afterward.

### 5.4 Preview system
`core/preview.rs` (plus `video.rs`, `audio.rs`, `mermaid.rs`,
`syntax_highlight.rs`) produces typed payloads rendered by
`containers/itemviewer_preview.rs`:
- Plain text (with encoding sniffing), including text extracted from **PDF**,
  **Word (.docx)**, and **EPUB** (reads the actual chapters in reading order,
  not the raw internal zip tree).
- **Syntax highlighting** (via `syntect`) for code files, and for fenced code
  blocks inside rendered Markdown, using the fence's own language tag.
- **Markdown**, rendered (headings, emphasis, lists, code blocks, block
  quotes, rules, task lists, simple tables), including fenced ` ```mermaid `
  blocks rendered as real flowcharts/sequence diagrams natively (no browser
  runtime), falling back to plain code for anything unsupported/malformed.
- Images, **animated GIF** playback, and **SVG** (rasterized, including
  gzip-compressed `.svgz`).
- **Video** playback (Media Foundation) with play/pause, seek slider, and
  current/total time; never auto-starts or loops.
- **Audio** playback (mp3/wav/flac/aac/m4a/wma/ogg/opus/aiff/alac) with a
  hand-drawn waveform, the same transport controls as video; never
  auto-starts or loops.
- **Archive** listing for zip *and* **7z**, with nesting and per-entry sizes.
- **Find-in-preview** — a find bar (auto-focused on open, with its own
  visible border and inline close) over Text/Markdown payloads: highlights
  every case-insensitive match and jumps to it, Next/Prev, match count.
- Text and Markdown previews are selectable, with **right-click to copy**
  the selection.

### 5.5 Search, filter & sorting
- **Type-to-filter** in the current directory (fuzzy, cached indices) — a
  visibly-bordered box with a leading search icon, auto-focused as soon as
  typing starts.
- **Global search** (search icon / Ctrl+F in the navigation bar of any pane)
  with a "This folder" (recursive) / "Everywhere" scope toggle. Powered by
  [voidtools Everything](https://www.voidtools.com/) when installed/running
  (near-instant, index-backed) with automatic fallback to a built-in
  filesystem search otherwise — the engine, default scope, and default view
  layout for new search tabs are all user-selectable in Settings → Behavior.
  Both engines understand `*`/`?` wildcards, plain keywords, and
  `key:value` filters (`ext:pdf`, `size:>10mb`, `modified:today`,
  `content:<word>` for in-file text search) combinable with free text.
  Results open as a normal tab compatible with every view mode, showing each
  match's actual folder; right-click a result for "Open File Location".
- **Saved Searches** — a dedicated sidebar section for one-click-saved
  queries + scope (capped at 50), reopened the same way a live search opens.
- Multi-column sort with persisted per-directory sort keys.

### 5.6 Organization
- **Favorites** — reorderable by drag, with custom icons (Phosphor icon
  picker with 1,500+ searchable glyphs, or a user-supplied image file copied
  into app data), and a dedicated management page in Settings.
- **Tags, including multi-tagging** — an item can belong to more than one
  tag group at once (a multi-select toggle picker, staying open across
  picks); tagged items are tinted in the file list and show every tag as a
  colored chip (with "+N" overflow) in a dedicated Tags column; a sidebar
  section lists tags; clicking one opens a dedicated tab listing every
  tagged item, fully sortable/reorderable/fit-to-content like other tables;
  removing a tag from within that tag's own view only removes that one tag.
- **Recent locations** — a sidebar section tracking the most-recently-
  visited real folders (capped at 20, excludes anything already favorited).
- Custom folder colors.

### 5.7 Theming & appearance
- Dark/light modes, instant toggle, each with its own fully independent
  palette.
- **Twelve prebuilt two-color presets** (accent + a color-theory-chosen
  contrasting secondary) applied in one click, and **user-created custom
  themes** — save your current colors under a name, apply/edit/delete them
  like a preset; saving now captures your *entire* edited palette (not just
  the two-color pair), so reselecting one restores everything, including
  after restarting the app.
- **Granular, per-surface color overrides** covering nearly every visible
  surface: core brand colors, rows/text, panels/icons, tabs, drive-usage
  bars, checkboxes/buttons (including a real disabled-state pair and
  primary-button text), corner radii, sidebar, status bar, address bar,
  search box, toolbar (background, icon, active-icon, and a *unified* hover
  color/background shared by every toolbar icon including the six
  view-layout icons), preview pane, the Settings sidebar's selected-item
  text/icon, the navigation completion toast, and notifications (including
  semantic green/yellow/red status colors) — a full hex/RGB/HSL color
  picker throughout.
- **Live Preview panel** in Settings → Appearance — a mockup of every major
  themed surface that updates instantly as a color is edited, including a
  genuinely interactive row for previewing real hover colors without
  leaving the page.
- **Layout options** — row density presets (Compact/Comfortable/Spacious)
  and a persisted sidebar width.
- Both light and dark palettes (plus custom themes) are exported/imported
  together as one JSON file, separate from a scoped custom-themes-only or
  custom-context-menu-only export.

### 5.8 File operation safety & productivity
- **Undo/Redo** (Ctrl+Z/Ctrl+Y/Ctrl+Shift+Z) for Rename, Bulk Rename, Move,
  and Copy — in-memory, capped at 50 entries. A Replace-resolved conflict is
  non-destructive (the overwritten item is recycled, not destroyed) and
  fully undoable/redoable.
- **Paste/drag-and-drop conflict resolution** — a themed Replace / Skip
  Existing / Rename dialog (both flows share one conflict-check + robocopy
  pipeline); Rename safely auto-numbers the new copy without ever
  overwriting the original mid-operation.
- **Bulk Rename** — multi-selecting and choosing Rename (or F2) opens a
  live-preview dialog: plain find/replace or full regex with capture
  groups, plus `{name}`/`{ext}`/`{n}`/`{n:03}` placeholders, cross-item and
  existing-file collision detection blocking the commit, applied as one
  batched, undoable native operation.
- **Checksums** — a context-menu entry computing CRC32/MD5/SHA-1/SHA-256 in
  one background pass, with per-hash copy buttons and a paste-to-verify
  Compare field.
- **Compress to zip** — a context-menu entry that archives the selection in
  the background, named after the item or shared parent folder.

### 5.9 System integration
- Network: `\\SERVER` / `\\SERVER\Share` address entry, a **Network**
  sidebar entry, and a **Shared Network** section that auto-discovers hosts
  advertising SMB shares via legacy NetBIOS browsing, an active subnet
  sweep, **mDNS** (`_smb._tcp` service discovery), and **WS-Discovery** —
  every candidate is still verified over SMB before it's shown.
- Portable devices (iPhone/Android/MTP) and raw/unmounted drive detection
  (ISO sticks, Linux partitions, with a widened filesystem allowlist
  covering exFAT/ReFS alongside NTFS/FAT32).
- CLI accepts real paths plus `::MY_PC::`, `::RECYCLE_BIN::`,
  `shell:MyComputerFolder`, `shell:RecycleBinFolder`, and general
  `shell:`/`::{GUID}` resolution through the real Windows shell namespace.
- Real Windows "This PC" shell icon; special icons for Desktop, Downloads,
  User Home, Music, Videos, Documents.
- Terminal launch in the current directory.

### 5.10 Persistence & settings
Each concern is stored as its own small binary/JSON file under
`%LOCALAPPDATA%\ExplorerEden\` (settings, favorites, tags, tab groups,
custom context menu, custom themes, session tabs, theme, sidebar sections,
window position). Settings UI is organized as General, Behavior, Startup &
Window, Appearance, Favorites, Custom Context Menu, Tab Groups, Tags,
Advanced. Behavior also covers double-click-navigates-up, an optional
selection-checkbox column, middle-click-opens-new-tab, whether last
session's tabs restore on launch, and default view layout/search scope for
new tabs. **Export/Import Settings** moves everything as one JSON file; the
custom context menu and custom themes each also have their own standalone,
scoped export/import.

### 5.11 Internationalization
Fluent (`.ftl`) bundles for `en-US`, `id-ID`, `ja-JP`, `zh-CN`, `zh-HK`,
`zh-TW`, embedded via `rust-embed`, with OS-locale detection at startup.

## 6. Non-goals

Inferred from current scope — **worth confirming with the maintainer** rather
than treating as settled:
- Cross-platform support (macOS/Linux). The product depends deeply on
  Win32/NT/Shell APIs; portability would mean a different product.
- Cloud storage sync/integration as a first-party feature.
- Being a file *editor* — preview is read-only by design.
- Telemetry or analytics of any kind, including opt-in.

## 7. Known gaps & quality risks

These are real, verified issues in the current build, not speculation:

1. **Shift+Delete's modifier is not reliably observed** — `i.modifiers.shift`
   has been measured as unset at the moment the `Delete` keypress is
   processed, despite the key genuinely being held. The context-menu
   "Delete Permanently" entry exists as the reliable path. Root cause
   (egui/winit modifier propagation vs. this app's input handling) is
   undiagnosed.
2. **Markdown right-click-copy over-includes** — the paragraph directly
   under the pointer contributes its whole text rather than only the
   selected portion, because egui collapses a label's selection on any
   button press before the widget reacts. Documented and accepted; a clean
   fix needs either a custom rich-text widget or upstream egui changes.
3. **No automated test coverage for UI/behavior.** Unit tests exist for a
   growing set of `core/`/pure-logic modules (100 tests as of this writing —
   pattern matching, search-query parsing, tag/recent-locations/saved-
   searches state, notification status mapping, syntax highlighting,
   archive listing, persistence round-trips) but nothing exercises the
   actual egui widget tree; every UI-facing regression is still caught by
   hand via the `eden-explorer-ui-test` skill.
4. **`postcard`-persisted structs have no per-field migration path.**
   Confirmed directly: appending a field to a struct like `ThemePalette` or
   `CustomThemeEntry` isn't safely additive the way `#[serde(default)]`
   implies — postcard's whole-struct decode has no per-field rescue, so an
   old save either fails outright (resetting to defaults) or, for anything
   without an explicit legacy-shape fallback, is silently lost. A one-off
   legacy-shadow-struct fallback now exists for `CustomThemeEntry`
   specifically (see `CLAUDE.md`); `ThemePalette` itself still has no such
   protection, so any future field added to it still resets a pre-existing
   `theme.bin` to defaults once on next load.
5. **i18n completeness is unenforced** — a key missing from a non-English
   locale fails silently. A checker script now exists
   (`.claude/skills/i18n-sync-check/`) but isn't wired into CI.

## 8. Planned enhancements **[Planned]**

From the README's own roadmap:
- Multi-tag **AND/OR filtering** in the tag view (multi-tagging itself has
  shipped — see §5.6 — this is specifically about filtering across more
  than one tag at once).

## 9. Proposed enhancements **[Proposed]**

Ideas for discussion, roughly ordered by value-to-effort within each theme.

### 9.1 Reliability & engineering health
- **Diagnose the Shift+Delete modifier issue properly.** Either fix the
  input path or, if it's an upstream egui/winit limitation, document it and
  make every modifier-dependent shortcut fall back to a menu equivalent
  consistently.
- **Wire `i18n-sync-check` into CI** so a PR that adds an untranslated key
  fails the build instead of shipping silently.
- **A UI regression harness.** The `eden-explorer-ui-test` skill already
  automates build → launch → click → screenshot; a small suite of scripted
  scenarios (open each view mode, rename, delete to bin, restore) run before
  release would catch the class of bug that currently only surfaces manually.
- **Crash/panic reporting that stays local** — write panics to a log file in
  app data with a "copy diagnostics" button, no network involved.

### 9.2 Search
Recursive/whole-drive search, content search, saved searches, and
attribute-filter syntax have all shipped (§5.5) — the remaining open idea:
- **A persistent index for the built-in engine.** The built-in fallback
  (used when Everything isn't installed/running) still does an on-demand
  walk per search; a small local index would close most of the remaining
  speed gap versus Everything for users who don't want a second app
  installed, at the cost of a background indexing process this project's
  "no background services" principle would need to explicitly accept.

### 9.3 Organization & workflow
Bulk rename, multi-tag filtering (now tracked in §8 as Planned), and
recent locations have all shipped (§5.6, §5.8) — still open:
- **Workspaces/sessions** — extend tab groups (§5.1) into named sessions
  that also capture split-pane state, view mode, and sort order, not just
  which folders are open.
- **Dual-pane sync** — synchronized scrolling and a folder-compare mode for
  the existing split view.

### 9.4 File operations
Undo/Redo, conflict-resolution UI, and a checksum panel have all shipped
(§5.8) — still open:
- **Operation queue with pause/resume** for long copies, plus an aggregate
  progress surface (the native dialogs are per-operation today).
- **RAR archive listing.** Deliberately skipped so far — the only practical
  Rust crate wraps the non-free-licensed official UnRAR library, which
  doesn't fit this project's open-source positioning; would need either a
  license exception decision or a from-scratch reader.

### 9.5 Preview
Syntax highlighting, SVG/EPUB/7z/audio support, and find-in-preview have
all shipped (§5.4) — still open:
- **A match strictly inside a fenced code block or table cell in the
  Markdown preview** is neither highlighted nor scrolled to by
  find-in-preview today (both render via a `Frame`/`Grid`, not the tracked
  `Label` path find-in-preview's highlighter walks) - a known, documented
  simplification from when that feature shipped.
- **Rich/rendered `.docx` preview** — currently text-extraction only (no
  formatting/images), same treatment as PDF.

### 9.6 Accessibility & input
- **AccessKit / screen-reader support** — egui has AccessKit integration
  available; the app currently exposes no accessibility tree, which also
  makes automated testing harder than it needs to be. This would help two
  problems at once.
- **Command palette** (Ctrl+Shift+P style) for keyboard-first users to reach
  any action or location without the mouse.
- **User-rebindable keyboard shortcuts**, persisted like other settings.

### 9.7 Extensibility
- **A plugin/extension surface.** Custom context-menu commands are already a
  primitive form of this; a documented manifest format (command + icon +
  match rules + optional preview handler) would let the community extend the
  app without forking it.

## 10. Success metrics **[Proposed]**

No telemetry means these have to be observable externally or measured
locally:
- **Adoption:** GitHub release download counts, stars, issue volume.
- **Performance (measured locally, per release):** cold-start to first
  interactive frame; time to list a 10k-entry directory; memory footprint at
  idle with 5 tabs open. A small reproducible benchmark script would make
  these comparable release-over-release — no such tooling exists in `src/`
  today (an earlier PRD draft's note about a stale README claim to this
  effect has since been corrected; this is now a forward-looking proposal,
  not a documentation-drift issue).
- **Quality:** open bug count and median time-to-close; number of releases
  requiring a same-week hotfix.
- **Reach:** locale coverage (currently 6) and share of untranslated keys
  (target: zero, enforced by CI).

## 11. Open questions

1. Is cross-platform support genuinely out of scope forever, or deferred?
   It affects how much Win32-specific coupling is acceptable in `core/`.
2. Should search grow into a persistent index (fast, but a background
   service and disk cost — arguably at odds with the "portable, no
   background services" principle) or stay on-demand?
3. Is a plugin model desirable, or does it conflict with the "no lock-in,
   minimal surface" principle?
4. What is the intended Windows 10 support commitment? The README hedges
   ("users reported no issues... cautious in declaring official support").
5. Should the app ever offer an installer/Store presence, or is portable-only
   a permanent stance?

---

*Companion documents: `CHECKLIST.md` (delivery status), `CLAUDE.md`
(engineering conventions and known traps), `.claude/skills/` (project
skills for testing, patterns, design review, i18n).*
