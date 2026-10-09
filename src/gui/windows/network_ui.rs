//! Settings > Network: the one place network locations are added, edited and
//! removed. The sidebar's own Shared Network section only lists what is
//! configured here (grouped the same way) and sends "Add"/"Edit" straight to
//! this page - there is no separate dialog any more.
//!
//! Master-detail layout: the left column lists every location grouped by
//! protocol (Windows shares first, then one group per remote kind that
//! actually has something in it); the right column is the form for whichever
//! entry is selected. Remote locations are tracked by their stable `id`; a
//! pinned share only has its position in the list, so the selection is
//! re-validated against the list every frame.

use crate::core::remote::RemoteKind;
use crate::core::utils::widgets::{eden_button, primary_dialog_button};
use crate::gui::i18n::I18n;
use crate::gui::theme::ThemePalette;
use crate::gui::windows::enums::SettingsAction;
use crate::gui::windows::network_location::{LocationType, NetworkLocationForm, NetworkSelection};
use crate::gui::windows::settings::{
    empty_state_hint, master_detail_column_size, no_selection_hint, setting_label,
};
use crate::gui::windows::structs::SettingsWindow;
use eframe::egui;
use egui_phosphor::regular;

/// The form for `selection`, or `None` if it no longer points at anything.
/// `New` always has a form - it is an entry being filled in, not a saved one.
fn form_for(settings: &SettingsWindow, selection: NetworkSelection) -> Option<NetworkLocationForm> {
    let prefs = &settings.current_settings.ui_prefs;
    match selection {
        NetworkSelection::New => Some(NetworkLocationForm::new()),
        NetworkSelection::Share(index) => prefs
            .network_places
            .get(index)
            .map(NetworkLocationForm::edit_share),
        NetworkSelection::Remote(id) => prefs
            .remote_connections
            .iter()
            .find(|c| c.id == id)
            .map(NetworkLocationForm::edit),
    }
}

/// Whether `selection` still points at something that exists.
///
/// Separate from `form_for` on purpose, and not written as
/// `form_for(..).is_some()`: this is checked **every frame** the page is
/// drawn, while building a form for a remote location calls
/// `credentials::load` - a Windows Credential Manager read. Answering "does
/// this still exist" was therefore hitting the credential store sixty times
/// a second for as long as the page stayed open.
fn selection_exists(settings: &SettingsWindow, selection: NetworkSelection) -> bool {
    let prefs = &settings.current_settings.ui_prefs;
    match selection {
        // Being filled in, so there is nothing for it to point at yet.
        NetworkSelection::New => true,
        NetworkSelection::Share(index) => index < prefs.network_places.len(),
        NetworkSelection::Remote(id) => prefs.remote_connections.iter().any(|c| c.id == id),
    }
}

/// Selects `selection` and rebuilds the detail pane's form for it.
pub(crate) fn select(settings: &mut SettingsWindow, selection: NetworkSelection) {
    settings.network_editor = form_for(settings, selection);
    settings.selected_network = settings.network_editor.is_some().then_some(selection);
}

pub fn draw_network_settings(
    ui: &mut egui::Ui,
    i18n: &I18n,
    settings: &mut SettingsWindow,
    palette: &ThemePalette,
) -> Option<SettingsAction> {
    let mut action = None;
    let ctx = ui.ctx().clone();

    setting_label(
        ui,
        &i18n.tr("settings_category_network"),
        Some((&i18n.tr("tooltip_settings_network"), palette)),
        palette,
    );
    ui.add_space(8.0);

    if eden_button(
        ui,
        palette,
        &format!("{} {}", regular::PLUS, i18n.tr("network_location_add")),
    )
    .clicked()
    {
        select(settings, NetworkSelection::New);
    }

    ui.add_space(10.0);

    // Drop a selection whose entry has gone (removed here, or a share index
    // left stale by a removal), then fall back to the first entry so the
    // detail pane isn't empty whenever there is something to show.
    if let Some(selection) = settings.selected_network
        && !selection_exists(settings, selection)
    {
        settings.selected_network = None;
        settings.network_editor = None;
    }
    if settings.selected_network.is_none() {
        let prefs = &settings.current_settings.ui_prefs;
        let first = if prefs.network_places.is_empty() {
            prefs
                .remote_connections
                .first()
                .map(|c| NetworkSelection::Remote(c.id))
        } else {
            Some(NetworkSelection::Share(0))
        };
        if let Some(selection) = first {
            select(settings, selection);
        }
    }

    let (col_w, col_h) = master_detail_column_size(ui);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(col_w, col_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("network_list_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if let Some(a) = draw_location_list(ui, i18n, settings, palette) {
                            action = Some(a);
                        }
                    });
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(col_w, col_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("network_detail_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let Some(form) = &mut settings.network_editor else {
                            no_selection_hint(
                                ui,
                                palette,
                                regular::GLOBE,
                                &i18n.tr("network_location_no_selection"),
                            );
                            return;
                        };
                        form.poll_test();

                        let title = if form.editing.is_some()
                            || matches!(settings.selected_network, Some(NetworkSelection::Share(_)))
                        {
                            "network_location_edit_title"
                        } else {
                            "network_location_add_title"
                        };
                        setting_label(
                            ui,
                            &i18n.tr(title),
                            Some((&i18n.tr("network_location_hint"), palette)),
                            palette,
                        );
                        ui.add_space(8.0);

                        draw_location_form(ui, i18n, form, palette);

                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if primary_dialog_button(ui, palette, &i18n.tr("save")).clicked() {
                                action = Some(SettingsAction::SaveNetworkLocation);
                            }
                            if matches!(form.kind, LocationType::Remote(_))
                                && ui
                                    .add_enabled_ui(!form.testing(), |ui| {
                                        eden_button(
                                            ui,
                                            palette,
                                            &i18n.tr("network_location_test"),
                                        )
                                    })
                                    .inner
                                    .clicked()
                            {
                                form.start_test(&ctx);
                            }
                            // Opening is a separate button now: the Settings
                            // page is itself a tab, so saving can't navigate
                            // the way the old dialog did without closing the
                            // page out from under the user.
                            if !matches!(settings.selected_network, Some(NetworkSelection::New))
                                && eden_button(ui, palette, &i18n.tr("network_location_open"))
                                    .clicked()
                            {
                                action = Some(SettingsAction::OpenNetworkLocation);
                            }
                        });
                    });
            },
        );
    });

    action
}

/// The left column: every configured location, grouped.
fn draw_location_list(
    ui: &mut egui::Ui,
    i18n: &I18n,
    settings: &mut SettingsWindow,
    palette: &ThemePalette,
) -> Option<SettingsAction> {
    let mut action = None;
    let mut reselect: Option<NetworkSelection> = None;

    let places = settings.current_settings.ui_prefs.network_places.clone();
    let connections = settings
        .current_settings
        .ui_prefs
        .remote_connections
        .clone();

    if places.is_empty() && connections.is_empty() {
        empty_state_hint(
            ui,
            palette,
            regular::GLOBE,
            &i18n.tr("network_location_empty_state"),
        );
        return None;
    }

    if !places.is_empty() {
        subsection_header(ui, palette, &i18n.tr("sidebar_network_group_windows"));
        for (index, place) in places.iter().enumerate() {
            let selected = settings.selected_network == Some(NetworkSelection::Share(index));
            let mut remove = false;
            let row = list_row(ui, palette, selected, |ui| {
                row_contents(
                    ui,
                    palette,
                    i18n,
                    regular::SHARE_NETWORK,
                    &place.name,
                    &place.path.display().to_string(),
                    &mut remove,
                );
            });
            if remove {
                action = Some(SettingsAction::RemoveNetworkPlace(index));
                settings.selected_network = None;
                settings.network_editor = None;
            } else if row.clicked() {
                reselect = Some(NetworkSelection::Share(index));
            }
        }
    }

    // One group per protocol, and only for protocols that actually have a
    // location saved - an empty group header would be pure chrome.
    for kind in RemoteKind::ALL {
        if !connections.iter().any(|conn| conn.kind == kind) {
            continue;
        }
        subsection_header(ui, palette, kind.label());
        for conn in connections.iter().filter(|conn| conn.kind == kind) {
            let selected = settings.selected_network == Some(NetworkSelection::Remote(conn.id));
            let mut remove = false;
            let detail = if kind == RemoteKind::S3 {
                conn.bucket.clone()
            } else {
                conn.host.clone()
            };
            let row = list_row(ui, palette, selected, |ui| {
                row_contents(
                    ui,
                    palette,
                    i18n,
                    crate::gui::windows::containers::sidebar::remote_glyph(kind),
                    &conn.display_name(),
                    &detail,
                    &mut remove,
                );
            });
            if remove {
                action = Some(SettingsAction::RemoveRemoteConnection(conn.id));
                settings.selected_network = None;
                settings.network_editor = None;
            } else if row.clicked() {
                reselect = Some(NetworkSelection::Remote(conn.id));
            }
        }
    }

    if let Some(selection) = reselect {
        select(settings, selection);
    }

    action
}

/// Icon, name, where it points, and a remove button. A plain `ui.label` here
/// would be selectable text and would steal the row's own clicks.
fn row_contents(
    ui: &mut egui::Ui,
    palette: &ThemePalette,
    i18n: &I18n,
    glyph: &str,
    name: &str,
    detail: &str,
    remove: &mut bool,
) {
    ui.add(egui::Label::new(glyph).selectable(false));
    ui.add(egui::Label::new(egui::RichText::new(name).strong()).selectable(false));
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if eden_button(ui, palette, regular::TRASH)
            .on_hover_text(i18n.tr("network_location_remove"))
            .clicked()
        {
            *remove = true;
        }
        if !detail.is_empty() {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(detail).color(palette.tooltip_text_color),
                )
                .truncate()
                .selectable(false),
            );
        }
    });
}

/// A muted group heading, matching the sidebar's own network subsections.
fn subsection_header(ui: &mut egui::Ui, palette: &ThemePalette, label: &str) {
    ui.add_space(6.0);
    ui.add(
        egui::Label::new(
            egui::RichText::new(label)
                .size(palette.text_size - 1.0)
                .color(palette.text_header_section.gamma_multiply(0.7)),
        )
        .selectable(false),
    );
    ui.add_space(4.0);
}

/// The add/edit form, moved here verbatim from the dialog this page replaced.
fn draw_location_form(
    ui: &mut egui::Ui,
    i18n: &I18n,
    form: &mut NetworkLocationForm,
    palette: &ThemePalette,
) {
    // Every field below is `desired_width(INFINITY)` - sized for the 460px
    // dialog this came from. Uncapped in a half-width settings column they
    // stretch all the way into the app's own outer window border.
    const FORM_MAX_WIDTH: f32 = 560.0;
    ui.set_max_width(ui.available_width().min(FORM_MAX_WIDTH));

    let label_width = 130.0;
    let row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
        ui.horizontal(|ui| {
            ui.add_sized(
                [label_width, 22.0],
                egui::Label::new(label).halign(egui::Align::Min),
            );
            add(ui);
        });
        ui.add_space(4.0);
    };
    let field = |ui: &mut egui::Ui, value: &mut String, hint: &str, password: bool| {
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
    let editing = form.editing.is_some();
    row(ui, &i18n.tr("network_location_type"), &mut |ui| {
        ui.add_enabled_ui(!editing, |ui| {
            egui::ComboBox::from_id_salt("network_location_type")
                .selected_text(type_label(form.kind))
                .width(220.0)
                .show_ui(ui, |ui| {
                    let mut types = vec![LocationType::Share];
                    types.extend(RemoteKind::ALL.map(LocationType::Remote));
                    for t in types {
                        if ui
                            .selectable_label(form.kind == t, type_label(t))
                            .clicked()
                            && form.kind != t
                        {
                            form.kind = t;
                            if let LocationType::Remote(kind) = t {
                                form.conn.kind = kind;
                                form.conn.port = kind.default_port(form.conn.https);
                            }
                            form.test = None;
                        }
                    }
                });
        });
    });

    match form.kind {
        LocationType::Share => {
            row(ui, &i18n.tr("network_location_folder"), &mut |ui| {
                field(ui, &mut form.share_path, r"\\server\share\folder", false)
            });
            row(ui, &i18n.tr("network_location_name"), &mut |ui| {
                field(ui, &mut form.share_name, "", false)
            });
        }
        LocationType::Remote(kind) => {
            row(ui, &i18n.tr("network_location_name"), &mut |ui| {
                field(
                    ui,
                    &mut form.conn.name,
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
                field(ui, &mut form.conn.host, server_hint, false)
            });
            row(ui, &i18n.tr("network_location_port"), &mut |ui| {
                ui.add(egui::DragValue::new(&mut form.conn.port).range(1..=65535));
                if matches!(kind, RemoteKind::WebDav | RemoteKind::S3) {
                    ui.add_space(12.0);
                    if ui.checkbox(&mut form.conn.https, "HTTPS").changed() {
                        form.conn.port = kind.default_port(form.conn.https);
                    }
                }
            });
            if kind == RemoteKind::S3 {
                row(ui, &i18n.tr("network_location_region"), &mut |ui| {
                    field(ui, &mut form.conn.region, "us-east-1", false)
                });
                row(ui, &i18n.tr("network_location_bucket"), &mut |ui| {
                    field(ui, &mut form.conn.bucket, "my-bucket", false)
                });
            }
            let user_label = if kind == RemoteKind::S3 {
                "network_location_access_key"
            } else {
                "network_location_user"
            };
            row(ui, &i18n.tr(user_label), &mut |ui| {
                field(ui, &mut form.conn.username, "", false)
            });
            if kind == RemoteKind::Sftp {
                row(ui, "", &mut |ui| {
                    ui.checkbox(&mut form.use_key_file, i18n.tr("network_location_use_key"));
                });
                if form.use_key_file {
                    row(ui, &i18n.tr("network_location_key_file"), &mut |ui| {
                        if eden_button(ui, palette, &i18n.tr("browse")).clicked()
                            && let Some(path) =
                                crate::gui::windows::windowsoverrides::dialog().pick_file()
                        {
                            form.key_file = path.display().to_string();
                        }
                        field(
                            ui,
                            &mut form.key_file,
                            r"C:\Users\me\.ssh\id_ed25519",
                            false,
                        );
                    });
                }
            }
            let secret_label = match kind {
                RemoteKind::S3 => "network_location_secret_key",
                RemoteKind::Sftp if form.use_key_file => "network_location_passphrase",
                _ => "network_location_password",
            };
            let secret_hint = if form.has_stored_password {
                i18n.tr("network_location_password_kept")
            } else {
                String::new()
            };
            row(ui, &i18n.tr(secret_label), &mut |ui| {
                field(ui, &mut form.password, &secret_hint, true)
            });
            let path_label = match kind {
                RemoteKind::WebDav => Some("network_location_path"),
                RemoteKind::Sftp | RemoteKind::Ftp | RemoteKind::Ftps => {
                    Some("network_location_start_folder")
                }
                RemoteKind::Redis => Some("network_location_key_prefix"),
                RemoteKind::S3 => None,
            };
            if let Some(label) = path_label {
                let hint = match kind {
                    RemoteKind::WebDav => "/remote.php/dav/files/me",
                    RemoteKind::Redis => "user:1000",
                    _ => "/home/me",
                };
                row(ui, &i18n.tr(label), &mut |ui| {
                    field(ui, &mut form.conn.path, hint, false)
                });
            }
            // Redis keeps several numbered databases; everything else has
            // nothing to pick between.
            if kind == RemoteKind::Redis {
                row(ui, &i18n.tr("network_location_database"), &mut |ui| {
                    ui.add(egui::DragValue::new(&mut form.conn.database).range(0..=15));
                });
                row(ui, "", &mut |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(i18n.tr("network_location_redis_read_only"))
                                .color(palette.tooltip_text_color),
                        )
                        .selectable(false),
                    );
                });
            }
            // SFTP only: the shell is a second channel on the same SSH
            // server, so there is nothing to offer for FTP/WebDAV/S3.
            if kind == RemoteKind::Sftp {
                row(ui, "", &mut |ui| {
                    ui.checkbox(&mut form.conn.shell, i18n.tr("network_location_shell"));
                });
            }
        }
    }

    if let Some(error) = &form.error {
        ui.colored_label(ui.visuals().warn_fg_color, error);
    }
    match &form.test {
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
        None if form.testing() => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(i18n.tr("network_location_testing"));
            });
        }
        None => {}
    }
}

/// A selectable row, sensed before its own content so the trash button still
/// wins its own pixels (see `send_to_ui`'s copy of this - each master-detail
/// page keeps its own by this codebase's convention).
fn list_row(
    ui: &mut egui::Ui,
    palette: &ThemePalette,
    selected: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    ui.spacing_mut().item_spacing.y = 0.0;

    let row_height = ui.spacing().interact_size.y + 20.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_height),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        let fill = if selected {
            palette.primary_hover
        } else {
            palette.row_bg
        };
        let stroke_color = if selected {
            palette.borders_active
        } else {
            palette.borders_default
        };
        let corner = egui::CornerRadius::same(palette.small_radius);
        ui.painter().rect_filled(rect, corner, fill);
        ui.painter().rect_stroke(
            rect,
            corner,
            egui::Stroke::new(1.5, stroke_color),
            egui::StrokeKind::Inside,
        );
    }

    let content_rect = rect.shrink2(egui::vec2(10.0, 6.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(content_rect), |ui| {
        ui.horizontal_centered(|ui| add_contents(ui));
    });

    ui.add_space(8.0);

    response
}
