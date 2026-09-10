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
`core/preview.rs` produces typed payloads rendered by
`containers/itemviewer_preview.rs`:
- Plain text (with encoding sniffing), including text extracted from **PDF**
  and **Word (.docx)** documents.
- **Markdown**, rendered (headings, emphasis, lists, code blocks, block
  quotes, rules, task lists, simple tables) rather than shown as source.
- Images, plus **animated GIF** playback.
- **Video** playback with play/pause and a seek slider (Media Foundation).
- **Archive** (zip-format) listing with nesting and per-entry sizes.
- Text and Markdown previews are selectable, with **right-click to copy**
  the selection.

### 5.5 Search, filter & sorting
- Type-to-filter in the current directory with fuzzy matching and cached
  indices.
- Multi-column sort with persisted per-directory sort keys.

### 5.6 Organization
- **Favorites** — reorderable by drag, with custom icons (Phosphor icon
  picker with 1,500+ searchable glyphs, or a user-supplied image file copied
  into app data), and a dedicated management page in Settings.
- **Tags** — colored tags on files/folders; tagged items are tinted in the
  file list; a sidebar section lists tags; clicking one opens a dedicated
  tab listing every tagged item with Name/Type/Location/Size/Modified
  columns (fully sortable/reorderable/fit-to-content like other tables).
- Custom folder colors.

### 5.7 Theming & appearance
- Dark/light modes with instant toggle.
- Full palette editor (`ThemePalette`): typography sizes, font family
  selection from system-installed fonts, brand/primary colors, surfaces,
  icon colors, row colors, borders.
- **Accent-driven retinting** — choosing an accent restyles sidebar,
  explorer, file list, tabs, toolbar icons, and borders together.
- Both light and dark palettes are exported/imported together.

### 5.8 System integration
- Network: `\\SERVER` / `\\SERVER\Share` address entry, a **Network**
  sidebar entry, and a **Shared Network** section that auto-discovers hosts
  advertising SMB shares via NetBIOS browsing.
- Portable devices (iPhone/Android/MTP) and raw/unmounted drive detection
  (ISO sticks, Linux partitions).
- CLI accepts real paths plus `::MY_PC::`, `::RECYCLE_BIN::`,
  `shell:MyComputerFolder`, `shell:RecycleBinFolder`.
- Real Windows "This PC" shell icon; special icons for Desktop, Downloads,
  User Home, Music, Videos, Documents.
- Terminal launch in the current directory.

### 5.9 Persistence & settings
Each concern is stored as its own small binary/JSON file under
`%LOCALAPPDATA%\ExplorerEden\` (settings, favorites, tags, tab groups,
custom context menu, session tabs, theme, sidebar sections, window
position). Settings UI is organized as General, Behavior, Startup & Window,
Appearance, Favorites, Custom Context Menu, Tab Groups, Tags, Advanced —
plus **Export/Import Settings** to move everything as one JSON file.

### 5.10 Internationalization
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
   few `core/` modules only; every UI-facing regression is currently caught
   by hand.
4. **Documentation drift** — the README claims a "built-in performance
   benchmarking system" (twice) but no such code exists in `src/`. Either
   the feature was removed or it was never shipped; the README should be
   corrected.
5. **i18n completeness is unenforced** — a key missing from a non-English
   locale fails silently. A checker script now exists
   (`.claude/skills/i18n-sync-check/`) but isn't wired into CI.

## 8. Planned enhancements **[Planned]**

From the README's own roadmap:
- Multi-tagging of objects (an item belonging to more than one tag).
- Enhanced theme customization: custom layouts, mix-and-match UI elements
  and styles.
- Additional network discovery protocols (mDNS / WS-Discovery) for devices
  that don't announce over legacy NetBIOS browsing.

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
- **Recursive / whole-drive search.** The current filter is
  current-directory only. This is the single biggest functional gap versus
  Explorer (and versus Everything, which many power users run alongside).
  Could start as an on-demand recursive walk with streaming results before
  considering a persistent index.
- **Content search** for text files, and filter-by-attribute syntax
  (`size:>100mb`, `ext:png`, `modified:today`).
- **Saved searches** surfaced in the sidebar next to tags.

### 9.3 Organization & workflow
- **Bulk rename** with pattern/regex preview — a standard power-user tool
  with no current equivalent in the app.
- **Workspaces/sessions** — extend tab groups into named sessions that also
  capture split-pane state, view mode, and sort order.
- **Dual-pane sync** — synchronized scrolling and a folder-compare mode for
  the existing split view.
- **Multi-tag filtering** — once multi-tagging lands, allow AND/OR filtering
  across tags in the tag view.
- **Recent locations** / frequently-visited list in the sidebar.

### 9.4 File operations
- **Operation queue with pause/resume** for long copies, plus an aggregate
  progress surface (the native dialogs are per-operation today).
- **Undo stack beyond the Recycle Bin** for moves and renames.
- **Conflict resolution UI** for same-name collisions with side-by-side
  metadata, rather than deferring entirely to the shell dialog.
- **Checksum / hash panel** in the properties or preview pane.

### 9.5 Preview
- **Syntax highlighting for code files** — the plumbing exists (a
  `TextEdit` with a custom `layouter` produces a styled galley over a single
  buffer), so this is mostly a tokenizer decision.
- **More formats:** SVG (a renderer is already a dependency), EPUB, 7z/rar
  archive listing, audio waveform + playback.
- **Preview-pane search** (find-in-preview) for long text documents.

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
  these comparable release-over-release — and would give the README's
  benchmarking claim something real to point at.
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
