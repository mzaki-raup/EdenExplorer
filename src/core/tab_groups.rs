//! User-defined "tab groups": a named, ordered list of folder entries the
//! user can open all at once as tabs - either added alongside the current
//! tabs or replacing them entirely. The same path may appear more than once
//! in a group (each occurrence opens as its own separate tab, not
//! deduplicated).
//!
//! Each entry can also carry a `split_path` - when set, opening that entry
//! opens one tab with a Secondary split-view pane already showing that
//! second folder, mirroring how a real dual-pane tab looks today. This lets
//! a group remember "these two folders side by side" as a single unit,
//! not just a flat list of single-pane tabs.
//!
//! Persisted separately from the rest of `AppSettings` (its own file, own
//! load/save functions), the same way favorites/tags/custom-context-menu
//! entries are.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct TabGroupEntry {
    pub path: PathBuf,
    /// When set, opening this entry opens one tab with a Secondary
    /// split-view pane at this path too (a dual-pane tab), instead of a
    /// plain single-pane tab.
    #[serde(default)]
    pub split_path: Option<PathBuf>,
}

impl TabGroupEntry {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            split_path: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct TabGroup {
    pub id: u64,
    pub name: String,
    /// Duplicates are allowed and meaningful: opening the group opens one
    /// tab per entry, even if the same path appears twice.
    #[serde(default)]
    pub entries: Vec<TabGroupEntry>,
}

impl TabGroup {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            name: String::new(),
            entries: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct TabGroupsSnapshot {
    #[serde(default)]
    groups: Vec<TabGroup>,
}

// --- Legacy shape, from before dual-pane entries existed (`paths: Vec<PathBuf>`
// instead of `entries: Vec<TabGroupEntry>`) - kept as a decode fallback so an
// existing user's saved tab groups don't silently disappear the first time
// they launch a build with this field. postcard's whole-struct decode has no
// per-field rescue (see `CLAUDE.md`'s notes on this), so a changed field
// shape has to be handled by trying the old shape explicitly, not by
// `#[serde(default)]` alone - mirrors `CustomThemeEntryLegacy` in
// `core/indexer.rs`.
#[derive(Serialize, Deserialize)]
struct TabGroupLegacy {
    id: u64,
    name: String,
    #[serde(default)]
    paths: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize, Default)]
struct TabGroupsSnapshotLegacy {
    #[serde(default)]
    groups: Vec<TabGroupLegacy>,
}

impl From<TabGroupsSnapshotLegacy> for TabGroupsSnapshot {
    fn from(legacy: TabGroupsSnapshotLegacy) -> Self {
        Self {
            groups: legacy
                .groups
                .into_iter()
                .map(|g| TabGroup {
                    id: g.id,
                    name: g.name,
                    entries: g.paths.into_iter().map(TabGroupEntry::new).collect(),
                })
                .collect(),
        }
    }
}

fn cache_path() -> Option<PathBuf> {
    let base = dirs::data_local_dir()?;
    Some(base.join("ExplorerEden").join("tab_groups.bin"))
}

pub fn load_tab_groups() -> Vec<TabGroup> {
    let Some(path) = cache_path() else {
        return Vec::new();
    };
    let Ok(data) = std::fs::read(&path) else {
        return Vec::new();
    };
    if let Ok(snapshot) = postcard::from_bytes::<TabGroupsSnapshot>(&data) {
        return snapshot.groups;
    }
    postcard::from_bytes::<TabGroupsSnapshotLegacy>(&data)
        .map(|legacy| TabGroupsSnapshot::from(legacy).groups)
        .unwrap_or_default()
}

pub fn save_tab_groups(groups: &[TabGroup]) {
    let Some(path) = cache_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let snapshot = TabGroupsSnapshot {
        groups: groups.to_vec(),
    };
    if let Ok(data) = postcard::to_allocvec(&snapshot) {
        let _ = std::fs::write(path, data);
    }
}

/// A fresh id, guaranteed higher than every group id currently in use, for a
/// newly-added group.
pub fn next_group_id(groups: &[TabGroup]) -> u64 {
    groups.iter().map(|g| g.id).max().unwrap_or(0) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_tab_group_bytes_without_split_path_still_decode() {
        let legacy = TabGroupsSnapshotLegacy {
            groups: vec![TabGroupLegacy {
                id: 1,
                name: "Dev".to_string(),
                paths: vec![PathBuf::from("C:\\a"), PathBuf::from("C:\\b")],
            }],
        };
        let bytes = postcard::to_allocvec(&legacy).unwrap();

        // The new shape must fail to decode legacy bytes (otherwise the
        // fallback below would never be reached in practice).
        assert!(postcard::from_bytes::<TabGroupsSnapshot>(&bytes).is_err());

        let decoded_legacy = postcard::from_bytes::<TabGroupsSnapshotLegacy>(&bytes).unwrap();
        let migrated: TabGroupsSnapshot = decoded_legacy.into();
        assert_eq!(migrated.groups.len(), 1);
        assert_eq!(migrated.groups[0].name, "Dev");
        assert_eq!(
            migrated.groups[0].entries,
            vec![
                TabGroupEntry::new(PathBuf::from("C:\\a")),
                TabGroupEntry::new(PathBuf::from("C:\\b")),
            ]
        );
    }

    #[test]
    fn tab_group_with_split_path_round_trips_through_postcard() {
        let groups = vec![TabGroup {
            id: 1,
            name: "Dev".to_string(),
            entries: vec![TabGroupEntry {
                path: PathBuf::from("C:\\a"),
                split_path: Some(PathBuf::from("C:\\b")),
            }],
        }];
        let bytes = postcard::to_allocvec(&TabGroupsSnapshot {
            groups: groups.clone(),
        })
        .unwrap();
        let decoded = postcard::from_bytes::<TabGroupsSnapshot>(&bytes).unwrap();
        assert_eq!(decoded.groups, groups);
    }
}
