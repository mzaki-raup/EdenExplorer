# EdenExplorer — Project Summary

## What it is
EdenExplorer is an open-source, high-performance file manager for Windows 10/11, built in Rust with [egui](https://github.com/emilk/egui/) (immediate-mode GUI). It's positioned as a fast, modern, FOSS alternative to Windows File Explorer, using direct NT-level filesystem APIs for performance instead of relying on higher-level Windows shell abstractions.

- **License:** MIT
- **Current version:** 0.27.0 (`Cargo.toml`)
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
  - `fs.rs` — filesystem operations
  - `indexer.rs` — directory scanning/indexing
  - `launch.rs` — launching files/programs
  - `portable.rs` — portable device support (iPhone, Android, etc.)
  - `utils/` — clipboard, colors, dialogs, file helpers, fonts, sorting, tabs, text, thumbnails, widgets
- **`gui/`** — UI layer:
  - `mod.rs`, `theme.rs`, `icons.rs`, `i18n.rs`, `utils.rs`, `dragdrop.rs`
  - `windows/` — application windows/panels: main window (+ impl split), navigation, settings, about, theme customization, drag-and-drop, shell context menu, Windows-specific overrides
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
- Dark/light themes with a full custom color palette editor
- Tagging system for files/folders with tag-colored views, plus custom folder colors
- Gallery/thumbnail view (small to extra-large) for media
- Context menu operations (cut/copy/paste/rename/delete), optional native Windows shell context menu integration
- Drag-and-drop both within the app and to/from native Windows (Explorer, Desktop, other apps)
- Portable device support and raw/unmounted drive (ISO, Linux partition) detection
- Native Recycle Bin support
- Multi-language UI (English, Indonesian, Japanese, Chinese variants) via Fluent i18n
- Persistent settings via an efficient binary cache; column sort/view state persists per directory
- Rich keyboard shortcut set (tab management, navigation, rename, properties, refresh, etc.)
- Six view modes (Details, Detail+Preview, Gallery, Columns, Column+Preview, Preview) with per-directory column layout, fit-to-content, and reordering
- Preview pane for text, Markdown (rendered), images, animated GIFs, video, PDF/Word text extraction, and zip archive listings — with selectable text and right-click copy
- Delete to Recycle Bin plus an explicit "Delete Permanently" action using the native Windows confirmation
- Explorer-style click-pause-click inline rename (extension excluded from the selection)

## Roadmap (from README)
Upcoming alpha features: multi-tagging of objects, deeper theme customization (custom layouts, mix/match UI), and additional network discovery protocols (mDNS/WS-Discovery). Detailed status lives in `CHECKLIST.md`; rationale and backlog ideas in `PRD.md`.

## Distribution
Distributed as a standalone Windows executable (no install) via GitHub Releases at `github.com/mtucciarone/EdenExplorer/releases`, with an automated release workflow (GitHub Actions + semantic-release).
