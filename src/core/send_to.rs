//! User-defined "Send To" groups: a named parent (with its own icon) holding
//! an ordered list of destination folders. Selecting a file/folder and
//! choosing one of these destinations from the right-click "Send To" submenu
//! **copies** the selection there - never moves it, so the original always
//! stays put no matter which destination is picked.
//!
//! Persisted separately from the rest of `AppSettings` (its own file, own
//! load/save functions), the same way favorites/tags/tab-groups/custom-
//! context-menu entries already are. The "show Send To in the context menu"
//! toggle lives in this same file rather than as a loose `AppSettings`
//! field, since it's meaningless without the groups it controls.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// How a Send To group's icon is drawn - mirrors `FavoriteItem`'s own
/// `custom_icon`/`custom_icon_file` pair (kept as one enum here instead of
/// two `Option` fields since a group's icon has no third "use the real
/// shell icon" state the way a favorite's folder does).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SendToIcon {
    /// No icon - just the label.
    None,
    /// A Phosphor icon glyph, picked from the shared searchable icon picker.
    Glyph(String),
    /// A user-browsed image file (.ico, .png, .jpg, ...), copied into the
    /// app's own data folder via `core::indexer::import_custom_icon` so it
    /// keeps working (and Export/Import Settings carries it along) even if
    /// the original file is moved or deleted.
    Custom(PathBuf),
}

impl Default for SendToIcon {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct SendToGroup {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub icon: SendToIcon,
    /// Duplicates are allowed on purpose, same as everywhere else in this
    /// app that lets a user build a list of paths (Tab Groups, Favorites).
    #[serde(default)]
    pub folders: Vec<PathBuf>,
}

impl SendToGroup {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            name: String::new(),
            icon: SendToIcon::None,
            folders: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct SendToSnapshot {
    #[serde(default)]
    groups: Vec<SendToGroup>,
    /// Whether the "Send To" submenu shows up in the file/folder right-click
    /// menu. Meaningless with zero groups - `save_send_to` doesn't enforce
    /// that itself (the settings page does, disabling the checkbox and
    /// resetting it to `false` when the last group is removed), but this
    /// stays a plain independent field rather than something derived, so a
    /// user who removes every group and re-adds one later doesn't have to
    /// re-enable it.
    #[serde(default)]
    context_menu_enabled: bool,
}

fn cache_path() -> Option<PathBuf> {
    let base = dirs::data_local_dir()?;
    Some(base.join("ExplorerEden").join("send_to.bin"))
}

/// Returns `(groups, context_menu_enabled)`.
pub fn load_send_to() -> (Vec<SendToGroup>, bool) {
    let Some(path) = cache_path() else {
        return (Vec::new(), false);
    };
    let Ok(data) = std::fs::read(&path) else {
        return (Vec::new(), false);
    };
    match postcard::from_bytes::<SendToSnapshot>(&data) {
        Ok(snapshot) => (snapshot.groups, snapshot.context_menu_enabled),
        Err(_) => (Vec::new(), false),
    }
}

pub fn save_send_to(groups: &[SendToGroup], context_menu_enabled: bool) {
    let Some(path) = cache_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let snapshot = SendToSnapshot {
        groups: groups.to_vec(),
        context_menu_enabled,
    };
    if let Ok(data) = postcard::to_allocvec(&snapshot) {
        let _ = std::fs::write(path, data);
    }
}

/// A fresh id, guaranteed higher than every group id currently in use, for a
/// newly-added group.
pub fn next_group_id(groups: &[SendToGroup]) -> u64 {
    groups.iter().map(|g| g.id).max().unwrap_or(0) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_to_snapshot_round_trips_through_postcard() {
        let groups = vec![SendToGroup {
            id: 1,
            name: "Backups".to_string(),
            icon: SendToIcon::Glyph("\u{E000}".to_string()),
            folders: vec![PathBuf::from("D:\\Backup"), PathBuf::from("E:\\Archive")],
        }];
        let bytes = postcard::to_allocvec(&SendToSnapshot {
            groups: groups.clone(),
            context_menu_enabled: true,
        })
        .unwrap();
        let decoded = postcard::from_bytes::<SendToSnapshot>(&bytes).unwrap();
        assert_eq!(decoded.groups, groups);
        assert!(decoded.context_menu_enabled);
    }

    #[test]
    fn next_group_id_is_higher_than_every_existing_id() {
        let groups = vec![SendToGroup::new(3), SendToGroup::new(1), SendToGroup::new(7)];
        assert_eq!(next_group_id(&groups), 8);
        assert_eq!(next_group_id(&[]), 1);
    }
}
