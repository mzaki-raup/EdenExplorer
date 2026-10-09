//! A network location: a remote one (SFTP, FTP, FTPS, WebDAV, S3) or a
//! network folder (`\\server\share`) pinned to the sidebar. Passwords go
//! straight to Windows Credential Manager; Test Connection signs in with
//! what's typed (in the background).
//!
//! This module holds the form's state and everything that happens when it is
//! saved or removed. The form itself is drawn by `network_ui`, which hosts it
//! in Settings > Network - the sidebar only lists what is already configured.

use crate::core::remote::{RemoteConnection, RemoteKind, credentials};
use crate::core::ui_prefs::NetworkPlace;
use crate::gui::windows::mainwindow::MainWindow;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

/// What the form is adding.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LocationType {
    Share,
    Remote(RemoteKind),
}

/// Which entry Settings > Network has selected. A remote location is tracked
/// by its stable id; a pinned share only has its position in the list, so the
/// page re-validates that index against the list every frame rather than
/// trusting it across a removal.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NetworkSelection {
    Share(usize),
    Remote(u64),
    /// Being filled in, not saved yet.
    New,
}

pub struct NetworkLocationForm {
    /// The remote location being edited (`None`: adding one).
    pub editing: Option<u64>,
    pub kind: LocationType,
    pub conn: RemoteConnection,
    pub share_path: String,
    pub share_name: String,
    pub password: String,
    /// Editing: the stored password is kept unless a new one is typed.
    pub has_stored_password: bool,
    pub use_key_file: bool,
    pub key_file: String,
    pub test: Option<Result<String, String>>,
    test_rx: Option<Receiver<Result<String, String>>>,
    pub error: Option<String>,
}

impl NetworkLocationForm {
    pub fn new() -> Self {
        Self {
            editing: None,
            kind: LocationType::Remote(RemoteKind::Sftp),
            conn: RemoteConnection::default(),
            share_path: String::new(),
            share_name: String::new(),
            password: String::new(),
            has_stored_password: false,
            use_key_file: false,
            key_file: String::new(),
            test: None,
            test_rx: None,
            error: None,
        }
    }

    pub fn edit(conn: &RemoteConnection) -> Self {
        Self {
            editing: Some(conn.id),
            kind: LocationType::Remote(conn.kind),
            conn: conn.clone(),
            has_stored_password: credentials::load(conn.id).is_some(),
            use_key_file: conn.key_file.is_some(),
            key_file: conn
                .key_file
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            ..Self::new()
        }
    }

    /// A pinned `\\server\share` folder being edited.
    pub fn edit_share(place: &NetworkPlace) -> Self {
        Self {
            kind: LocationType::Share,
            share_path: place.path.display().to_string(),
            share_name: place.name.clone(),
            ..Self::new()
        }
    }

    /// The connection as filled in (validated).
    fn connection(&self) -> Result<RemoteConnection, String> {
        let LocationType::Remote(kind) = self.kind else {
            return Err("Not a remote location".into());
        };
        let mut conn = self.conn.clone();
        conn.kind = kind;
        conn.host = conn.host.trim().trim_end_matches('/').to_string();
        // Accept "https://host/path" pasted into the server box.
        for scheme in ["https://", "http://", "sftp://", "ftps://", "ftp://"] {
            if let Some(rest) = conn.host.strip_prefix(scheme) {
                if scheme == "http://" {
                    conn.https = false;
                }
                let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
                if !path.is_empty() && conn.path.is_empty() {
                    conn.path = format!("/{path}");
                }
                conn.host = host.to_string();
            }
        }
        if let Some((host, port)) = conn.host.rsplit_once(':')
            && let Ok(port) = port.parse()
        {
            conn.host = host.to_string();
            conn.port = port;
        }
        conn.key_file =
            (kind == RemoteKind::Sftp && self.use_key_file && !self.key_file.trim().is_empty())
                .then(|| PathBuf::from(self.key_file.trim()));
        if kind == RemoteKind::S3 {
            if conn.bucket.trim().is_empty() {
                return Err("Enter the bucket name.".into());
            }
        } else if conn.host.is_empty() {
            return Err("Enter the server's name or address.".into());
        }
        if conn.port == 0 {
            conn.port = kind.default_port(conn.https);
        }
        Ok(conn)
    }

    /// The password to sign in with now: the typed one, or the stored one.
    fn secret(&self) -> String {
        if self.password.is_empty() && self.has_stored_password {
            self.editing.and_then(credentials::load).unwrap_or_default()
        } else {
            self.password.clone()
        }
    }

    /// Signs in with what's typed, on a background thread.
    pub fn start_test(&mut self, ctx: &egui::Context) {
        match self.connection() {
            Ok(conn) => {
                let (tx, rx) = channel();
                let secret = self.secret();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let start = crate::core::remote::remote_path(&conn.start_segments());
                    let result = crate::core::remote::connect_with_secret(&conn, &secret)
                        .and_then(|mut fs| fs.list(&start))
                        .map(|items| format!("Connected ({} items).", items.len()));
                    let _ = tx.send(result);
                    ctx.request_repaint();
                });
                self.test = None;
                self.error = None;
                self.test_rx = Some(rx);
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Picks up a finished test; call once per frame while the form is shown.
    pub fn poll_test(&mut self) {
        if let Some(rx) = &self.test_rx
            && let Ok(result) = rx.try_recv()
        {
            self.test = Some(result);
            self.test_rx = None;
        }
    }

    pub fn testing(&self) -> bool {
        self.test_rx.is_some()
    }
}

impl MainWindow {
    fn save_remote_connections(&mut self) {
        let prefs = &self.settings_window.current_settings.ui_prefs;
        crate::core::remote::set_connections(&prefs.remote_connections);
        crate::core::ui_prefs::save_ui_prefs(prefs);
    }

    /// Removes a remote location (and its stored password and host key).
    pub(crate) fn remove_remote_connection(&mut self, id: u64) {
        let prefs = &mut self.settings_window.current_settings.ui_prefs;
        if let Some(conn) = prefs.remote_connections.iter().find(|c| c.id == id) {
            credentials::forget_host_key(&conn.host, conn.port);
        }
        prefs.remote_connections.retain(|c| c.id != id);
        credentials::remove(id);
        crate::core::remote::disconnect(id);
        crate::core::remote::forget_local_copies(id);
        self.save_remote_connections();
    }

    pub(crate) fn remove_network_place(&mut self, index: usize) {
        let prefs = &mut self.settings_window.current_settings.ui_prefs;
        if index < prefs.network_places.len() {
            prefs.network_places.remove(index);
            crate::core::ui_prefs::save_ui_prefs(prefs);
        }
    }

    /// Saves whatever the Settings > Network form has open. Unlike the modal
    /// this replaced, it does not navigate anywhere - the Settings page is
    /// itself the current tab, so opening the location would close the page
    /// the user is still working in.
    /// Returns how to keep the saved entry selected.
    pub(crate) fn apply_network_location(
        &mut self,
        form: &NetworkLocationForm,
    ) -> Result<NetworkSelection, String> {
        if form.kind == LocationType::Share {
            let path = form.share_path.trim().trim_end_matches('\\').to_string();
            if path.is_empty() {
                return Err("Enter the folder's path.".into());
            }
            let path = PathBuf::from(path);
            let name = if form.share_name.trim().is_empty() {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string())
            } else {
                form.share_name.trim().to_string()
            };
            let prefs = &mut self.settings_window.current_settings.ui_prefs;
            prefs.network_places.retain(|p| p.path != path);
            prefs.network_places.push(NetworkPlace {
                name,
                path: path.clone(),
            });
            crate::core::ui_prefs::save_ui_prefs(prefs);
            return Ok(NetworkSelection::Share(prefs.network_places.len() - 1));
        }
        let mut conn = form.connection()?;
        let prefs = &mut self.settings_window.current_settings.ui_prefs;
        conn.id = match form.editing {
            Some(id) => id,
            None => {
                prefs
                    .remote_connections
                    .iter()
                    .map(|c| c.id)
                    .max()
                    .unwrap_or(0)
                    + 1
            }
        };
        if !form.password.is_empty() || !form.has_stored_password {
            credentials::store(conn.id, &form.password)
                .map_err(|e| format!("Couldn't save the password: {e}"))?;
        }
        match prefs
            .remote_connections
            .iter_mut()
            .find(|c| c.id == conn.id)
        {
            Some(existing) => *existing = conn.clone(),
            None => prefs.remote_connections.push(conn.clone()),
        }
        // Reconnect with the new settings next time.
        crate::core::remote::disconnect(conn.id);
        self.save_remote_connections();
        Ok(NetworkSelection::Remote(conn.id))
    }

    /// Opens `path` in the focused pane.
    pub(crate) fn open_path_in_current_view(&mut self, path: PathBuf) {
        self.current_nav_mut().go_to(path);
        self.mark_tab_infos_dirty();
        self.load_path();
    }
}
