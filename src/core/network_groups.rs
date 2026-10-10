//! The order the sidebar's Network section draws its groups in, and the order
//! of the locations inside each one.
//!
//! Two separate things, both editable in Settings > Network:
//!
//! * **Group order** - Windows shares, SFTP, FTP, FTPS, WebDAV, S3, Redis.
//!   Saved as ids, read back with [`ordered`], which follows the same rules as
//!   `sidebar_sections`: an id that is no longer a real group is dropped, and
//!   a group the saved order does not mention is appended rather than slotted
//!   into its default place - a saved order is an explicit choice, and a new
//!   protocol should not shove someone's arrangement around.
//! * **Order within a group** - the saved locations themselves. A connection's
//!   group is decided by its `kind`, so moving one "up" means swapping it with
//!   the nearest connection of the *same* kind, not with whatever happens to
//!   sit next to it in the list (see [`move_connection`]).

use crate::core::remote::{RemoteConnection, RemoteKind};

/// One group of the Network section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkGroup {
    /// Windows file sharing: the saved `\\server\share` places, plus whatever
    /// is discovered on the local network.
    Windows,
    /// One protocol's saved connections.
    Remote(RemoteKind),
}

impl NetworkGroup {
    pub const ALL: [NetworkGroup; 7] = [
        NetworkGroup::Windows,
        NetworkGroup::Remote(RemoteKind::Sftp),
        NetworkGroup::Remote(RemoteKind::Ftp),
        NetworkGroup::Remote(RemoteKind::Ftps),
        NetworkGroup::Remote(RemoteKind::WebDav),
        NetworkGroup::Remote(RemoteKind::S3),
        NetworkGroup::Remote(RemoteKind::Redis),
    ];

    /// The id saved to disk. Stable on purpose: renaming a variant must not
    /// reshuffle everyone's sidebar.
    pub fn id(self) -> &'static str {
        match self {
            NetworkGroup::Windows => "windows",
            NetworkGroup::Remote(RemoteKind::Sftp) => "sftp",
            NetworkGroup::Remote(RemoteKind::Ftp) => "ftp",
            NetworkGroup::Remote(RemoteKind::Ftps) => "ftps",
            NetworkGroup::Remote(RemoteKind::WebDav) => "webdav",
            NetworkGroup::Remote(RemoteKind::S3) => "s3",
            NetworkGroup::Remote(RemoteKind::Redis) => "redis",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        NetworkGroup::ALL.into_iter().find(|g| g.id() == id)
    }
}

/// The groups in the order they should be drawn.
pub fn ordered(saved: &[String]) -> Vec<NetworkGroup> {
    let mut order: Vec<NetworkGroup> = Vec::with_capacity(NetworkGroup::ALL.len());
    for id in saved {
        if let Some(group) = NetworkGroup::from_id(id)
            && !order.contains(&group)
        {
            order.push(group);
        }
    }
    for group in NetworkGroup::ALL {
        if !order.contains(&group) {
            order.push(group);
        }
    }
    order
}

/// `ordered`'s result as ids, for saving.
pub fn ids(order: &[NetworkGroup]) -> Vec<String> {
    order.iter().map(|g| g.id().to_string()).collect()
}

/// Moves one group one place up or down. `false` when it is already at that
/// end, so a caller can leave its button disabled.
pub fn move_group(order: &mut [NetworkGroup], group: NetworkGroup, up: bool) -> bool {
    let Some(at) = order.iter().position(|g| *g == group) else {
        return false;
    };
    let to = if up {
        if at == 0 {
            return false;
        }
        at - 1
    } else {
        if at + 1 >= order.len() {
            return false;
        }
        at + 1
    };
    order.swap(at, to);
    true
}

/// Moves the connection `id` one place within its own protocol group.
///
/// The list holds every protocol's connections mixed together and the sidebar
/// filters it per group, so "up" has to mean "before the previous connection
/// of this same kind" - swapping with the immediate neighbour would move it
/// past a connection of another protocol and look like nothing happened.
pub fn move_connection(connections: &mut [RemoteConnection], id: u64, up: bool) -> bool {
    let Some(at) = connections.iter().position(|c| c.id == id) else {
        return false;
    };
    let kind = connections[at].kind;
    let neighbour = if up {
        connections[..at].iter().rposition(|c| c.kind == kind)
    } else {
        connections[at + 1..]
            .iter()
            .position(|c| c.kind == kind)
            .map(|offset| at + 1 + offset)
    };
    let Some(to) = neighbour else {
        return false;
    };
    connections.swap(at, to);
    true
}

/// Moves a saved Windows share one place. Returns where it ended up, so a
/// selection tracking it by index can follow.
pub fn move_place<T>(places: &mut [T], index: usize, up: bool) -> Option<usize> {
    if index >= places.len() {
        return None;
    }
    let to = if up {
        index.checked_sub(1)?
    } else if index + 1 < places.len() {
        index + 1
    } else {
        return None;
    };
    places.swap(index, to);
    Some(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(id: u64, kind: RemoteKind, name: &str) -> RemoteConnection {
        RemoteConnection {
            id,
            kind,
            name: name.to_string(),
            ..Default::default()
        }
    }

    fn names(connections: &[RemoteConnection]) -> Vec<&str> {
        connections.iter().map(|c| c.name.as_str()).collect()
    }

    #[test]
    fn nothing_saved_gives_the_default_order() {
        assert_eq!(ordered(&[]), NetworkGroup::ALL.to_vec());
    }

    #[test]
    fn a_saved_order_is_honoured() {
        let saved = ids(&[
            NetworkGroup::Remote(RemoteKind::Redis),
            NetworkGroup::Windows,
            NetworkGroup::Remote(RemoteKind::S3),
        ]);
        let order = ordered(&saved);
        assert_eq!(order[0], NetworkGroup::Remote(RemoteKind::Redis));
        assert_eq!(order[1], NetworkGroup::Windows);
        assert_eq!(order[2], NetworkGroup::Remote(RemoteKind::S3));
        // Everything it didn't mention is still there, after what it did.
        assert_eq!(order.len(), NetworkGroup::ALL.len());
    }

    #[test]
    fn an_unknown_id_is_ignored_and_a_repeat_does_not_duplicate() {
        let saved = vec![
            "sftp".to_string(),
            "gopher".to_string(),
            "sftp".to_string(),
        ];
        let order = ordered(&saved);
        assert_eq!(order[0], NetworkGroup::Remote(RemoteKind::Sftp));
        assert_eq!(order.len(), NetworkGroup::ALL.len());
    }

    #[test]
    fn a_group_added_in_a_later_release_appears_at_the_end() {
        // Someone who arranged their groups before Redis existed: it must show
        // up rather than vanish, and must not shove their own order around.
        let saved: Vec<String> = NetworkGroup::ALL
            .iter()
            .filter(|g| **g != NetworkGroup::Remote(RemoteKind::Redis))
            .map(|g| g.id().to_string())
            .collect();
        let order = ordered(&saved);
        assert_eq!(order.len(), NetworkGroup::ALL.len());
        assert_eq!(
            *order.last().unwrap(),
            NetworkGroup::Remote(RemoteKind::Redis)
        );
    }

    #[test]
    fn moving_a_group_stops_at_each_end() {
        let mut order = NetworkGroup::ALL.to_vec();
        assert!(!move_group(&mut order, NetworkGroup::Windows, true));
        assert!(move_group(&mut order, NetworkGroup::Windows, false));
        assert_eq!(order[0], NetworkGroup::Remote(RemoteKind::Sftp));
        assert_eq!(order[1], NetworkGroup::Windows);

        let last = *NetworkGroup::ALL.last().unwrap();
        let mut order = NetworkGroup::ALL.to_vec();
        assert!(!move_group(&mut order, last, false));
    }

    #[test]
    fn a_connection_moves_past_its_own_protocol_not_its_neighbour() {
        // Interleaved on purpose: the immediate neighbour of `sftp-b` is an
        // FTP connection, and swapping with that would reorder nothing the
        // user can see.
        let mut connections = vec![
            conn(1, RemoteKind::Sftp, "sftp-a"),
            conn(2, RemoteKind::Ftp, "ftp-a"),
            conn(3, RemoteKind::Sftp, "sftp-b"),
        ];
        assert!(move_connection(&mut connections, 3, true));
        assert_eq!(names(&connections), vec!["sftp-b", "ftp-a", "sftp-a"]);
        // The FTP connection stayed where it was, between them.
        assert_eq!(connections[1].kind, RemoteKind::Ftp);
    }

    #[test]
    fn a_connection_at_the_end_of_its_group_does_not_move() {
        let mut connections = vec![
            conn(1, RemoteKind::Sftp, "only-sftp"),
            conn(2, RemoteKind::Ftp, "ftp-a"),
            conn(3, RemoteKind::Ftp, "ftp-b"),
        ];
        // Alone in its group: nowhere to go in either direction, even though
        // there are connections above and below it in the list.
        assert!(!move_connection(&mut connections, 1, true));
        assert!(!move_connection(&mut connections, 1, false));
        // Last of its own group, with one above it.
        assert!(!move_connection(&mut connections, 3, false));
        assert!(move_connection(&mut connections, 3, true));
        assert_eq!(names(&connections), vec!["only-sftp", "ftp-b", "ftp-a"]);
    }

    #[test]
    fn moving_an_unknown_connection_changes_nothing() {
        let mut connections = vec![conn(1, RemoteKind::Sftp, "a")];
        assert!(!move_connection(&mut connections, 99, true));
        assert_eq!(names(&connections), vec!["a"]);
    }

    #[test]
    fn a_place_reports_where_it_landed_so_a_selection_can_follow() {
        let mut places = vec!["a", "b", "c"];
        assert_eq!(move_place(&mut places, 2, true), Some(1));
        assert_eq!(places, vec!["a", "c", "b"]);
        assert_eq!(move_place(&mut places, 0, true), None);
        assert_eq!(move_place(&mut places, 2, false), None);
        assert_eq!(move_place(&mut places, 9, true), None);
        assert_eq!(places, vec!["a", "c", "b"]);
    }
}
