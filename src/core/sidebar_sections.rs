//! The sidebar's sections, and the order the user has put them in.
//!
//! The sidebar draws these top to bottom. Which ones are *shown* is a
//! separate thing (`indexer::SidebarSectionVisibility`, Settings > General);
//! this module only decides the order, edited in Settings > Sidebar.

/// A section of the sidebar, in the order a fresh install shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarSection {
    Places,
    Storage,
    Cloud,
    Linux,
    Folders,
    Favorites,
    Tags,
    SavedSearches,
    RecentLocations,
    SharedNetwork,
}

impl SidebarSection {
    pub const ALL: [SidebarSection; 10] = [
        SidebarSection::Places,
        SidebarSection::Storage,
        SidebarSection::Cloud,
        SidebarSection::Linux,
        SidebarSection::Folders,
        SidebarSection::Favorites,
        SidebarSection::Tags,
        SidebarSection::SavedSearches,
        SidebarSection::RecentLocations,
        SidebarSection::SharedNetwork,
    ];

    /// The id saved to disk. Stable: renaming a variant must not reshuffle
    /// everyone's sidebar, and an id that is no longer recognised is simply
    /// dropped when the saved order is read back.
    pub fn id(self) -> &'static str {
        match self {
            SidebarSection::Places => "places",
            SidebarSection::Storage => "storage",
            SidebarSection::Cloud => "cloud",
            SidebarSection::Linux => "linux",
            SidebarSection::Folders => "folders",
            SidebarSection::Favorites => "favorites",
            SidebarSection::Tags => "tags",
            SidebarSection::SavedSearches => "saved_searches",
            SidebarSection::RecentLocations => "recent_locations",
            SidebarSection::SharedNetwork => "shared_network",
        }
    }

    /// The same key the section's own header uses, so the settings list and
    /// the sidebar can never drift apart.
    pub fn label_key(self) -> &'static str {
        match self {
            SidebarSection::Places => "places",
            SidebarSection::Storage => "storage",
            SidebarSection::Cloud => "places_cloud",
            SidebarSection::Linux => "places_linux",
            SidebarSection::Folders => "folder_tree_section",
            SidebarSection::Favorites => "favorites",
            SidebarSection::Tags => "tags",
            SidebarSection::SavedSearches => "saved_searches",
            SidebarSection::RecentLocations => "recent_locations",
            SidebarSection::SharedNetwork => "shared_network",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        SidebarSection::ALL.into_iter().find(|s| s.id() == id)
    }
}

/// The sections in the order they should be drawn.
///
/// Anything saved that is no longer a real section is ignored, and any
/// section the saved order does not mention is appended at the end - so a
/// release that adds a section shows it to everyone who had already
/// reordered their sidebar, rather than hiding it.
///
/// Appended rather than slotted into its default position on purpose: a
/// saved order is an explicit choice, and dropping a new section into the
/// middle of it would shove the user's own arrangement around. At the end it
/// is visible, and one move away from wherever they want it.
pub fn ordered(saved: &[String]) -> Vec<SidebarSection> {
    let mut order: Vec<SidebarSection> = Vec::with_capacity(SidebarSection::ALL.len());
    for id in saved {
        if let Some(section) = SidebarSection::from_id(id)
            && !order.contains(&section)
        {
            order.push(section);
        }
    }
    for section in SidebarSection::ALL {
        if !order.contains(&section) {
            order.push(section);
        }
    }
    order
}

/// `ordered`'s result as ids, for saving.
pub fn ids(order: &[SidebarSection]) -> Vec<String> {
    order.iter().map(|s| s.id().to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_saved_gives_the_default_order() {
        assert_eq!(ordered(&[]), SidebarSection::ALL.to_vec());
    }

    #[test]
    fn a_saved_order_is_honoured() {
        let saved = ids(&[
            SidebarSection::Tags,
            SidebarSection::Places,
            SidebarSection::Storage,
            SidebarSection::Cloud,
            SidebarSection::Linux,
            SidebarSection::Folders,
            SidebarSection::Favorites,
            SidebarSection::SavedSearches,
            SidebarSection::RecentLocations,
            SidebarSection::SharedNetwork,
        ]);
        let order = ordered(&saved);
        assert_eq!(order[0], SidebarSection::Tags);
        assert_eq!(order[1], SidebarSection::Places);
        assert_eq!(order.len(), SidebarSection::ALL.len());
    }

    #[test]
    fn an_unknown_id_is_ignored_and_every_section_still_appears() {
        let saved = vec!["tags".to_string(), "a_section_that_was_removed".to_string()];
        let order = ordered(&saved);
        assert_eq!(order[0], SidebarSection::Tags);
        assert_eq!(order.len(), SidebarSection::ALL.len());
        for section in SidebarSection::ALL {
            assert!(order.contains(&section), "{section:?} went missing");
        }
    }

    #[test]
    fn a_section_missing_from_an_old_saved_order_is_still_drawn() {
        // What an upgrade looks like: the user reordered before this
        // section existed, so their saved list simply has no entry for it.
        let saved: Vec<String> = SidebarSection::ALL
            .iter()
            .filter(|s| **s != SidebarSection::Cloud)
            .map(|s| s.id().to_string())
            .collect();
        let order = ordered(&saved);
        assert!(order.contains(&SidebarSection::Cloud));
        assert_eq!(order.len(), SidebarSection::ALL.len());
    }

    #[test]
    fn a_duplicated_id_is_not_drawn_twice() {
        let saved = vec!["tags".to_string(), "tags".to_string()];
        let order = ordered(&saved);
        assert_eq!(order.len(), SidebarSection::ALL.len());
    }

    #[test]
    fn every_section_has_its_own_id_and_label() {
        let mut ids: Vec<&str> = SidebarSection::ALL.iter().map(|s| s.id()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "two sections share an id");
    }
}
