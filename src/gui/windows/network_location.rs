//! The Add/Edit Network Location dialog: a remote location (SFTP, FTP,
//! FTPS, WebDAV, S3) or a network folder (`\\server\share`) pinned to the
//! sidebar. Passwords go straight to Windows Credential Manager; Test
//! Connection signs in with what's typed (in the background).

use crate::core::remote::{RemoteConnection, RemoteKind, credentials};
use crate::core::ui_prefs::NetworkPlace;
use crate::gui::windows::mainwindow::MainWindow;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

/// What the dialog is adding.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LocationType {
    Share,
    Remote(RemoteKind),
}

pub struct NetworkLocationDialog {
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

impl NetworkLocationDialog {
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
}

enum Outcome {
    Save,
    Cancel,
}

impl MainWindow {
    pub(crate) fn open_network_location_dialog(&mut self, editing: Option<u64>) {
        let existing = editing.and_then(|id| {
            self.settings_window
                .current_settings
                .ui_prefs
                .remote_connections
                .iter()
                .find(|c| c.id == id)
                .cloned()
        });
        self.network_location_dialog = Some(match existing {
            Some(conn) => NetworkLocationDialog::edit(&conn),
            None => NetworkLocationDialog::new(),
        });
    }

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

    pub(crate) fn draw_network_location_dialog(
        &mut self,
        ctx: &egui::Context,
        palette: &crate::gui::theme::ThemePalette,
    ) {
        use crate::core::utils::widgets::{
            eden_button, modal_frame, modal_icon_header, primary_dialog_button,
        };
        use egui_phosphor::regular;

        let Some(mut dlg) = self.network_location_dialog.take() else {
            return;
        };
        if let Some(rx) = &dlg.test_rx
            && let Ok(result) = rx.try_recv()
        {
            dlg.test = Some(result);
            dlg.test_rx = None;
        }
        let i18n = &self.i18n;
        let mut outcome: Option<Outcome> = None;
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            outcome = Some(Outcome::Cancel);
        }

        egui::Area::new(egui::Id::new("network_location_scrim"))
            .order(egui::Order::Middle)
            .interactable(true)
            .show(ctx, |ui| {
                let rect = ctx.content_rect();
                ui.painter()
                    .rect_filled(rect, 0.0, palette.modal_background_effect_color);
                ui.interact(rect, ui.id().with("block"), egui::Sense::click());
            });

        // The dialog sits just above its scrim, below Foreground, so the
        // Type combo's popup (a Foreground layer) always shows over it.
        let scrim =
            egui::LayerId::new(egui::Order::Middle, egui::Id::new("network_location_scrim"));
        let area = egui::LayerId::new(egui::Order::Middle, egui::Id::new("network_location_area"));
        ctx.set_sublayer(scrim, area);
        egui::Area::new(egui::Id::new("network_location_area"))
            .order(egui::Order::Middle)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                modal_frame(&ctx.style_of(ctx.theme()), palette).show(ui, |ui| {
                    ui.set_width(460.0);
                    let title = if dlg.editing.is_some() {
                        "network_location_edit_title"
                    } else {
                        "network_location_add_title"
                    };
                    modal_icon_header(
                        ui,
                        palette,
                        regular::GLOBE,
                        palette.primary,
                        &i18n.tr(title),
                        Some(&i18n.tr("network_location_hint")),
                    );
                    ui.add_space(12.0);

                    let label_width = 130.0;
                    let row =
                        |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [label_width, 22.0],
                                    egui::Label::new(label).halign(egui::Align::Min),
                                );
                                add(ui);
                            });
                            ui.add_space(4.0);
                        };
                    let field =
                        |ui: &mut egui::Ui, value: &mut String, hint: &str, password: bool| {
                            ui.add(
                                egui::TextEdit::singleline(value)
                                    .hint_text(hint)
                                    .password(password)
                                    .desired_width(f32::INFINITY),
                            );
                        };

                    // Type.
                    let type_label = |t: LocationType| match t {
                        LocationType::Share => i18n.tr("network_location_type_share"),
                        LocationType::Remote(kind) => kind.label().to_string(),
                    };
                    let editing = dlg.editing.is_some();
                    row(ui, &i18n.tr("network_location_type"), &mut |ui| {
                        ui.add_enabled_ui(!editing, |ui| {
                            egui::ComboBox::from_id_salt("network_location_type")
                                .selected_text(type_label(dlg.kind))
                                .width(260.0)
                                .show_ui(ui, |ui| {
                                    let mut types = vec![LocationType::Share];
                                    types.extend(RemoteKind::ALL.map(LocationType::Remote));
                                    for t in types {
                                        if ui
                                            .selectable_label(dlg.kind == t, type_label(t))
                                            .clicked()
                                            && dlg.kind != t
                                        {
                                            dlg.kind = t;
                                            if let LocationType::Remote(kind) = t {
                                                dlg.conn.kind = kind;
                                                dlg.conn.port = kind.default_port(dlg.conn.https);
                                            }
                                            dlg.test = None;
                                        }
                                    }
                                });
                        });
                    });

                    match dlg.kind {
                        LocationType::Share => {
                            row(ui, &i18n.tr("network_location_folder"), &mut |ui| {
                                field(ui, &mut dlg.share_path, r"\\server\share\folder", false)
                            });
                            row(ui, &i18n.tr("network_location_name"), &mut |ui| {
                                field(ui, &mut dlg.share_name, "", false)
                            });
                        }
                        LocationType::Remote(kind) => {
                            row(ui, &i18n.tr("network_location_name"), &mut |ui| {
                                field(
                                    ui,
                                    &mut dlg.conn.name,
                                    &i18n.tr("network_location_name_hint"),
                                    false,
                                )
                            });
                            let server_label = if kind == RemoteKind::S3 {
                                "network_location_endpoint"
                            } else {
                                "network_location_server"
                            };
                            let server_hint = if kind == RemoteKind::S3 {
                                "s3.example.com (empty = Amazon S3)"
                            } else {
                                "example.com"
                            };
                            row(ui, &i18n.tr(server_label), &mut |ui| {
                                field(ui, &mut dlg.conn.host, server_hint, false)
                            });
                            row(ui, &i18n.tr("network_location_port"), &mut |ui| {
                                ui.add(egui::DragValue::new(&mut dlg.conn.port).range(1..=65535));
                                if matches!(kind, RemoteKind::WebDav | RemoteKind::S3) {
                                    ui.add_space(12.0);
                                    if ui.checkbox(&mut dlg.conn.https, "HTTPS").changed() {
                                        dlg.conn.port = kind.default_port(dlg.conn.https);
                                    }
                                }
                            });
                            if kind == RemoteKind::S3 {
                                row(ui, &i18n.tr("network_location_region"), &mut |ui| {
                                    field(ui, &mut dlg.conn.region, "us-east-1", false)
                                });
                                row(ui, &i18n.tr("network_location_bucket"), &mut |ui| {
                                    field(ui, &mut dlg.conn.bucket, "my-bucket", false)
                                });
                            }
                            let user_label = if kind == RemoteKind::S3 {
                                "network_location_access_key"
                            } else {
                                "network_location_user"
                            };
                            row(ui, &i18n.tr(user_label), &mut |ui| {
                                field(ui, &mut dlg.conn.username, "", false)
                            });
                            if kind == RemoteKind::Sftp {
                                row(ui, "", &mut |ui| {
                                    ui.checkbox(
                                        &mut dlg.use_key_file,
                                        i18n.tr("network_location_use_key"),
                                    );
                                });
                                if dlg.use_key_file {
                                    row(ui, &i18n.tr("network_location_key_file"), &mut |ui| {
                                        if eden_button(ui, palette, &i18n.tr("browse")).clicked()
                                            && let Some(path) =
                                                crate::gui::windows::windowsoverrides::dialog()
                                                    .pick_file()
                                        {
                                            dlg.key_file = path.display().to_string();
                                        }
                                        field(
                                            ui,
                                            &mut dlg.key_file,
                                            r"C:\Users\me\.ssh\id_ed25519",
                                            false,
                                        );
                                    });
                                }
                            }
                            let secret_label = match kind {
                                RemoteKind::S3 => "network_location_secret_key",
                                RemoteKind::Sftp if dlg.use_key_file => {
                                    "network_location_passphrase"
                                }
                                _ => "network_location_password",
                            };
                            let secret_hint = if dlg.has_stored_password {
                                i18n.tr("network_location_password_kept")
                            } else {
                                String::new()
                            };
                            row(ui, &i18n.tr(secret_label), &mut |ui| {
                                field(ui, &mut dlg.password, &secret_hint, true)
                            });
                            let path_label = match kind {
                                RemoteKind::WebDav => Some("network_location_path"),
                                RemoteKind::Sftp | RemoteKind::Ftp | RemoteKind::Ftps => {
                                    Some("network_location_start_folder")
                                }
                                RemoteKind::S3 => None,
                            };
                            if let Some(label) = path_label {
                                let hint = if kind == RemoteKind::WebDav {
                                    "/remote.php/dav/files/me"
                                } else {
                                    "/home/me"
                                };
                                row(ui, &i18n.tr(label), &mut |ui| {
                                    field(ui, &mut dlg.conn.path, hint, false)
                                });
                            }
                        }
                    }

                    if let Some(error) = &dlg.error {
                        ui.colored_label(ui.visuals().warn_fg_color, error);
                    }
                    match &dlg.test {
                        Some(Ok(text)) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(110, 190, 120),
                                format!("{} {text}", regular::CHECK_CIRCLE),
                            );
                        }
                        Some(Err(text)) => {
                            ui.colored_label(
                                ui.visuals().warn_fg_color,
                                format!("{} {text}", regular::WARNING),
                            );
                        }
                        None if dlg.test_rx.is_some() => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(i18n.tr("network_location_testing"));
                            });
                        }
                        None => {}
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if primary_dialog_button(ui, palette, &i18n.tr("save")).clicked() {
                            outcome = Some(Outcome::Save);
                        }
                        if matches!(dlg.kind, LocationType::Remote(_))
                            && ui
                                .add_enabled_ui(dlg.test_rx.is_none(), |ui| {
                                    eden_button(ui, palette, &i18n.tr("network_location_test"))
                                })
                                .inner
                                .clicked()
                        {
                            match dlg.connection() {
                                Ok(conn) => {
                                    let (tx, rx) = channel();
                                    let secret = dlg.secret();
                                    let ctx = ctx.clone();
                                    std::thread::spawn(move || {
                                        let start = crate::core::remote::remote_path(
                                            &conn.start_segments(),
                                        );
                                        let result = crate::core::remote::connect_with_secret(
                                            &conn, &secret,
                                        )
                                        .and_then(|mut fs| fs.list(&start))
                                        .map(|items| format!("Connected ({} items).", items.len()));
                                        let _ = tx.send(result);
                                        ctx.request_repaint();
                                    });
                                    dlg.test = None;
                                    dlg.error = None;
                                    dlg.test_rx = Some(rx);
                                }
                                Err(e) => dlg.error = Some(e),
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if eden_button(ui, palette, &i18n.tr("cancel")).clicked() {
                                outcome = Some(Outcome::Cancel);
                            }
                        });
                    });
                });
            });

        match outcome {
            Some(Outcome::Cancel) => {}
            Some(Outcome::Save) => {
                if let Err(e) = self.apply_network_location(&dlg) {
                    dlg.error = Some(e);
                    self.network_location_dialog = Some(dlg);
                }
            }
            None => self.network_location_dialog = Some(dlg),
        }
    }

    fn apply_network_location(&mut self, dlg: &NetworkLocationDialog) -> Result<(), String> {
        if dlg.kind == LocationType::Share {
            let path = dlg.share_path.trim().trim_end_matches('\\').to_string();
            if path.is_empty() {
                return Err("Enter the folder's path.".into());
            }
            let path = PathBuf::from(path);
            let name = if dlg.share_name.trim().is_empty() {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string())
            } else {
                dlg.share_name.trim().to_string()
            };
            let prefs = &mut self.settings_window.current_settings.ui_prefs;
            prefs.network_places.retain(|p| p.path != path);
            prefs.network_places.push(NetworkPlace {
                name,
                path: path.clone(),
            });
            crate::core::ui_prefs::save_ui_prefs(prefs);
            self.open_path_in_current_view(path);
            return Ok(());
        }
        let mut conn = dlg.connection()?;
        let prefs = &mut self.settings_window.current_settings.ui_prefs;
        conn.id = match dlg.editing {
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
        if !dlg.password.is_empty() || !dlg.has_stored_password {
            credentials::store(conn.id, &dlg.password)
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
        self.open_path_in_current_view(crate::core::remote::path_for(
            conn.id,
            &conn.start_segments(),
        ));
        Ok(())
    }

    /// Opens `path` in the focused pane.
    pub(crate) fn open_path_in_current_view(&mut self, path: PathBuf) {
        self.current_nav_mut().go_to(path);
        self.mark_tab_infos_dirty();
        self.load_path();
    }
}
