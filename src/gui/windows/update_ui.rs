//! The "a newer release is available" surface: the status bar's indicator and
//! the modal behind it.
//!
//! `core::release` decides whether a release is worth offering and
//! `core::update` does the fetching/verifying/installing; everything here is
//! presentation plus the few clicks that drive them.
//!
//! The status bar lives in `containers::explorer`, several call sites deep from
//! here, so the indicator is passed through an `egui::Context` memory flag
//! rather than a new `draw_tab_content` parameter - the same mechanism
//! `BLOCKING_MODAL_MEMORY_ID` already uses, and for the same reason.

use crate::core::release::Update;
use crate::core::update::{self, Event};
use std::path::PathBuf;

/// Set once per frame to the version string of an available update, read by the
/// status bar.
pub const UPDATE_BADGE_MEMORY_ID: &str = "update_available_version";
/// Set by the status bar when its indicator is clicked, read back here.
pub const UPDATE_CLICKED_MEMORY_ID: &str = "update_indicator_clicked";

/// Where the update flow has got to. One `MainWindow` field rather than several
/// loose ones, so "is the modal up" is a single question.
#[derive(Default)]
pub struct UpdateUiState {
    /// Offered but not yet acted on.
    pub available: Option<Update>,
    /// Downloaded and checked against its published checksum, ready to install.
    pub downloaded: Option<(Update, PathBuf)>,
    /// A download is in flight.
    pub downloading: bool,
    /// Installed; only a restart is left.
    pub installed: bool,
    /// Whether the modal is on screen. Kept separate from `available` so the
    /// user can dismiss the modal and still have the status bar indicator.
    pub modal_open: bool,
    /// A failure, or the answer to a check the user asked for. Shown in the
    /// modal; already user-facing text.
    pub message: Option<String>,
}

impl UpdateUiState {
    /// Whether anything here should suppress the item viewer's own keyboard
    /// shortcuts this frame (see `BLOCKING_MODAL_MEMORY_ID`).
    pub fn blocks_input(&self) -> bool {
        self.modal_open
    }
}

impl super::mainwindow::MainWindow {
    /// Drains what the background check/download threads finished, once per
    /// frame.
    pub fn poll_update_events(&mut self) {
        for event in update::drain_events() {
            match event {
                Event::Available(found) => {
                    // A check the user asked for opens the modal; an automatic
                    // one only lights the status bar, so it never steals focus
                    // from whatever they were doing.
                    let manual = self.update_state.message.is_some() || self.update_state.modal_open;
                    self.update_state.available = Some(found);
                    self.update_state.message = None;
                    if manual {
                        self.update_state.modal_open = true;
                    }
                }
                Event::UpToDate => {
                    self.update_state.available = None;
                    self.update_state.message = Some(self.i18n.tr("update_up_to_date"));
                    self.update_state.modal_open = true;
                }
                Event::Failed(error) => {
                    self.update_state.downloading = false;
                    self.update_state.message =
                        Some(format!("{}: {error}", self.i18n.tr("update_failed")));
                    self.update_state.modal_open = true;
                }
                Event::Downloaded { update, file } => {
                    self.update_state.downloading = false;
                    self.update_state.downloaded = Some((update, file));
                    self.update_state.modal_open = true;
                }
            }
        }

        // The status bar's indicator, and a click on it.
        let badge = self
            .update_state
            .available
            .as_ref()
            .map(|u| u.version.to_string());
        let clicked = self.egui_ctx.as_ref().is_some_and(|ctx| {
            ctx.memory_mut(|mem| {
                let id = egui::Id::new(UPDATE_CLICKED_MEMORY_ID);
                let was = mem.data.get_temp::<bool>(id).unwrap_or(false);
                mem.data.insert_temp(id, false);
                was
            })
        });
        if let Some(ctx) = self.egui_ctx.as_ref() {
            ctx.memory_mut(|mem| {
                mem.data
                    .insert_temp(egui::Id::new(UPDATE_BADGE_MEMORY_ID), badge);
            });
        }
        if clicked {
            self.update_state.modal_open = true;
        }
    }

    /// Starts a check the user asked for. Reports whatever it finds, including
    /// "nothing", unlike the automatic one.
    pub fn check_for_updates_now(&mut self) {
        self.update_state.message = Some(self.i18n.tr("update_checking"));
        self.update_state.modal_open = true;
        update::check(true);
    }

    /// Draws the update modal - a no-op unless something opened it. Follows
    /// `draw_select_by_pattern_modal`'s `Area` + `Frame::popup` structure (not
    /// `egui::Window`, whose remembered-size behaviour this app has already been
    /// bitten by - see `CLAUDE.md`).
    pub fn draw_update_modal(
        &mut self,
        ctx: &egui::Context,
        palette: &crate::gui::theme::ThemePalette,
    ) {
        use crate::core::utils::widgets::{
            eden_button, modal_frame, modal_icon_header, primary_dialog_button,
        };
        use egui_phosphor::regular;

        if !self.update_state.modal_open {
            return;
        }

        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let mut start_download = false;
        let mut do_install = false;
        let mut do_skip = false;
        let mut open_page: Option<String> = None;

        egui::Area::new(egui::Id::new("update_scrim"))
            .order(egui::Order::Middle)
            .interactable(true)
            .show(ctx, |ui| {
                let rect = ctx.content_rect();
                ui.painter()
                    .rect_filled(rect, 0.0, palette.modal_background_effect_color);
                if ui
                    .interact(rect, ui.id().with("click"), egui::Sense::click())
                    .clicked()
                {
                    close = true;
                }
            });

        egui::Area::new(egui::Id::new("update_area"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                modal_frame(&ctx.style_of(ctx.theme()), palette).show(ui, |ui| {
                    ui.set_width(460.0);

                    let running = crate::core::release::Version::running();

                    // One modal covers every state the flow can be in, so the
                    // header says which one.
                    let (icon, title, subtitle) = if self.update_state.installed {
                        (
                            regular::CHECK_CIRCLE,
                            self.i18n.tr("update_installed_title"),
                            Some(self.i18n.tr("update_installed_hint")),
                        )
                    } else if self.update_state.downloaded.is_some() {
                        (
                            regular::DOWNLOAD_SIMPLE,
                            self.i18n.tr("update_ready_title"),
                            Some(self.i18n.tr("update_ready_hint")),
                        )
                    } else if let Some(found) = &self.update_state.available {
                        (
                            regular::ARROW_CIRCLE_UP,
                            self.i18n.tr("update_available_title"),
                            Some(format!("v{running}  \u{2192}  v{}", found.version)),
                        )
                    } else {
                        (
                            regular::INFO,
                            self.i18n.tr("about_current_version"),
                            Some(format!("v{running}")),
                        )
                    };
                    modal_icon_header(
                        ui,
                        palette,
                        icon,
                        palette.primary,
                        &title,
                        subtitle.as_deref(),
                    );
                    ui.add_space(12.0);

                    if let Some(message) = &self.update_state.message {
                        ui.label(
                            egui::RichText::new(message)
                                .size(palette.text_size)
                                .color(palette.text_normal),
                        );
                        ui.add_space(10.0);
                    }

                    // The release notes, as the forge has them. Capped so a
                    // long changelog cannot grow the modal past the window.
                    if let Some(found) = &self.update_state.available
                        && !found.notes.is_empty()
                        && self.update_state.downloaded.is_none()
                        && !self.update_state.installed
                    {
                        egui::ScrollArea::vertical()
                            .max_height(180.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(&found.notes)
                                        .size(palette.text_size - 1.0)
                                        .color(palette.text_normal.gamma_multiply(0.85)),
                                );
                            });
                        ui.add_space(12.0);
                    }

                    if self.update_state.downloading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(
                                egui::RichText::new(self.i18n.tr("update_downloading"))
                                    .size(palette.text_size)
                                    .color(palette.text_normal),
                            );
                        });
                        ui.add_space(12.0);
                    }

                    // A release with no SHA256SUMS.txt is never installed
                    // automatically - say so rather than offering a button
                    // that would only fail.
                    let unverifiable = self
                        .update_state
                        .available
                        .as_ref()
                        .is_some_and(|u| u.sums.is_none());
                    if unverifiable && self.update_state.downloaded.is_none() {
                        ui.label(
                            egui::RichText::new(self.i18n.tr("update_no_checksums"))
                                .size(palette.text_size - 1.0)
                                .color(palette.notification_status_warning),
                        );
                        ui.add_space(12.0);
                    }

                    ui.horizontal(|ui| {
                        if self.update_state.installed {
                            if primary_dialog_button(ui, palette, &self.i18n.tr("update_restart_now"))
                                .clicked()
                            {
                                update::restart();
                            }
                        } else if self.update_state.downloaded.is_some() {
                            if primary_dialog_button(ui, palette, &self.i18n.tr("update_install"))
                                .clicked()
                            {
                                do_install = true;
                            }
                        } else if self.update_state.available.is_some() {
                            if ui
                                .add_enabled_ui(
                                    !self.update_state.downloading && !unverifiable,
                                    |ui| {
                                        primary_dialog_button(
                                            ui,
                                            palette,
                                            &self.i18n.tr("update_download"),
                                        )
                                    },
                                )
                                .inner
                                .clicked()
                            {
                                start_download = true;
                            }
                            if eden_button(ui, palette, &self.i18n.tr("update_release_page")).clicked()
                            {
                                open_page = self
                                    .update_state
                                    .available
                                    .as_ref()
                                    .map(|u| u.page.clone());
                            }
                            if eden_button(ui, palette, &self.i18n.tr("update_skip")).clicked() {
                                do_skip = true;
                            }
                        } else if eden_button(ui, palette, &self.i18n.tr("update_check_now")).clicked()
                        {
                            self.update_state.message = Some(self.i18n.tr("update_checking"));
                            update::check(true);
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if eden_button(ui, palette, &self.i18n.tr("close")).clicked() {
                                close = true;
                            }
                        });
                    });
                });
            });

        if start_download
            && let Some(found) = self.update_state.available.clone()
        {
            self.update_state.downloading = true;
            self.update_state.message = None;
            update::start_download(found);
        }

        if do_install
            && let Some((found, file)) = self.update_state.downloaded.clone()
        {
            match update::install(&file) {
                Ok(()) => {
                    self.update_state.installed = true;
                    self.update_state.available = None;
                    self.update_state.message = None;
                    let _ = found;
                }
                Err(error) => {
                    self.update_state.message =
                        Some(format!("{}: {error}", self.i18n.tr("update_failed")));
                }
            }
        }

        if do_skip
            && let Some(found) = self.update_state.available.take()
        {
            update::skip(&found);
            close = true;
        }

        if let Some(page) = open_page {
            crate::core::launch::open_url(&page);
        }

        if close {
            self.update_state.modal_open = false;
            self.update_state.message = None;
        }
    }
}
