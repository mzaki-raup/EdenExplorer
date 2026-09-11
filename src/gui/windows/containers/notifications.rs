use crate::core::utils::widgets::{clickable_icon, eden_button};
use crate::gui::i18n::I18n;
use crate::gui::theme::ThemePalette;
use eframe::egui;
use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, RichText, Stroke};
use egui_phosphor::regular;
use std::time::{Duration, Instant};

/// How long a completion toast stays on screen before auto-hiding.
const TOAST_DURATION: Duration = Duration::from_secs(4);
const PANEL_WIDTH: f32 = 320.0;
const PANEL_MAX_HEIGHT: f32 = 340.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOpKind {
    Copy,
    Move,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOpStatus {
    InProgress,
    Completed,
    Failed,
    /// Not produced yet - today's synchronous `IFileOperation` calls have no
    /// cancel path. Reserved for the robocopy-backed engine, which will let
    /// an in-progress operation actually be cancelled from the panel.
    #[allow(dead_code)]
    Cancelled,
}

/// One entry in the notification list - a single copy/move/delete operation,
/// from the moment it's kicked off through however it ends. Phase A wires
/// this to the existing `IFileOperation`-backed engine (see
/// `mainwindow_imp.rs`): operations that already run on a background thread
/// (paste/cut-move) get a real `InProgress` -> `Completed` transition,
/// while ones that still run synchronously (delete, drag-drop moves) are
/// recorded already-finished, since the native OS dialog already covers
/// their in-flight progress for now.
pub struct FileOperation {
    pub id: u64,
    pub kind: FileOpKind,
    pub status: FileOpStatus,
    pub item_count: usize,
    /// Human-readable destination/context, e.g. a folder name or "Recycle Bin" -
    /// already localized/formatted by the caller, not a raw path (paths can be
    /// long and this is meant to fit a narrow notification row).
    pub destination_label: String,
    /// Reserved for a future elapsed-time/duration display in the panel.
    #[allow(dead_code)]
    pub started_at: Instant,
    pub finished_at: Option<Instant>,
}

struct ActiveToast {
    op_id: u64,
    shown_at: Instant,
}

#[derive(Default)]
pub struct NotificationsState {
    /// Newest-first.
    pub operations: Vec<FileOperation>,
    pub panel_open: bool,
    next_id: u64,
    active_toast: Option<ActiveToast>,
}

impl NotificationsState {
    /// Records a new operation as `InProgress` and returns its id, to be
    /// passed to `finish_operation` once it completes.
    pub fn start_operation(
        &mut self,
        kind: FileOpKind,
        item_count: usize,
        destination_label: String,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.operations.insert(
            0,
            FileOperation {
                id,
                kind,
                status: FileOpStatus::InProgress,
                item_count,
                destination_label,
                started_at: Instant::now(),
                finished_at: None,
            },
        );
        id
    }

    /// Records an operation that already ran to completion synchronously
    /// (no separate `start_operation` call) - e.g. delete, or a drag-drop
    /// move that still blocks the UI thread today.
    pub fn record_finished(
        &mut self,
        kind: FileOpKind,
        item_count: usize,
        destination_label: String,
        status: FileOpStatus,
    ) -> u64 {
        let id = self.start_operation(kind, item_count, destination_label);
        self.finish_operation(id, status);
        id
    }

    pub fn finish_operation(&mut self, id: u64, status: FileOpStatus) {
        if let Some(op) = self.operations.iter_mut().find(|o| o.id == id) {
            op.status = status;
            op.finished_at = Some(Instant::now());
        }
        self.active_toast = Some(ActiveToast {
            op_id: id,
            shown_at: Instant::now(),
        });
    }

    pub fn in_progress_count(&self) -> usize {
        self.operations
            .iter()
            .filter(|o| o.status == FileOpStatus::InProgress)
            .count()
    }

    /// Drops every tracked operation, including ones still `InProgress` -
    /// this only clears them from the panel/badge, it doesn't touch the
    /// underlying `IFileOperation` call actually running on its background
    /// thread. If that thread later calls `finish_operation` for an id that
    /// was cleared here, it's a harmless no-op (see that function).
    pub fn clear_all(&mut self) {
        self.operations.clear();
    }
}

fn kind_icon(kind: FileOpKind) -> &'static str {
    match kind {
        FileOpKind::Copy => regular::COPY,
        FileOpKind::Move => regular::ARROWS_LEFT_RIGHT,
        FileOpKind::Delete => regular::TRASH,
    }
}

fn status_icon(status: FileOpStatus) -> &'static str {
    match status {
        FileOpStatus::InProgress => regular::SPINNER,
        FileOpStatus::Completed => regular::CHECK_CIRCLE,
        FileOpStatus::Failed => regular::WARNING_CIRCLE,
        FileOpStatus::Cancelled => regular::X_CIRCLE,
    }
}

/// Reuses the theme's own semantic colors (the same ones the drive-usage
/// bar picks from) rather than hardcoding red/green, so a notification
/// row's status color follows the user's chosen accent/palette instead of
/// looking the same regardless of theme.
fn status_color(status: FileOpStatus, palette: &ThemePalette) -> Color32 {
    match status {
        FileOpStatus::InProgress => palette.primary,
        FileOpStatus::Completed => palette.drive_usage_normal,
        FileOpStatus::Failed => palette.drive_usage_critical,
        FileOpStatus::Cancelled => palette.icon_color,
    }
}

fn kind_label(i18n: &I18n, kind: FileOpKind) -> String {
    match kind {
        FileOpKind::Copy => i18n.tr("notifications_kind_copy"),
        FileOpKind::Move => i18n.tr("notifications_kind_move"),
        FileOpKind::Delete => i18n.tr("notifications_kind_delete"),
    }
}

fn status_label(i18n: &I18n, status: FileOpStatus) -> String {
    match status {
        FileOpStatus::InProgress => i18n.tr("notifications_status_in_progress"),
        FileOpStatus::Completed => i18n.tr("notifications_status_completed"),
        FileOpStatus::Failed => i18n.tr("notifications_status_failed"),
        FileOpStatus::Cancelled => i18n.tr("notifications_status_cancelled"),
    }
}

fn item_word(i18n: &I18n, count: usize) -> String {
    if count == 1 {
        i18n.tr("item_capital")
    } else {
        i18n.tr("items_capital")
    }
}

/// e.g. "Copying 12 Items to Downloads" / "Deleting 3 Items".
fn operation_title(i18n: &I18n, op: &FileOperation) -> String {
    let verb = kind_label(i18n, op.kind);
    let items = item_word(i18n, op.item_count);
    if op.destination_label.is_empty() {
        format!("{verb} {} {items}", op.item_count)
    } else {
        format!(
            "{verb} {} {items} {} {}",
            op.item_count,
            i18n.tr("notifications_to"),
            op.destination_label
        )
    }
}

/// Draws the bell icon button (with an in-progress badge) and, when clicked,
/// its dropdown panel listing every tracked operation. Call once, anywhere a
/// single icon-sized widget would go - designed for the app's top-right
/// window-controls row.
pub fn draw_notifications_button(
    ui: &mut egui::Ui,
    i18n: &I18n,
    palette: &ThemePalette,
    state: &mut NotificationsState,
) {
    let in_progress = state.in_progress_count();
    let icon = if in_progress > 0 {
        regular::BELL_RINGING
    } else {
        regular::BELL
    };

    // A few px of breathing room on the right, inside the bell's own slot -
    // the in-progress badge extends past the icon's own glyph, and without
    // this it can crowd right up against whatever sits immediately to the
    // right of this widget (the window's minimize button, in practice).
    ui.add_space(4.0);

    let resp = clickable_icon(ui, icon, palette)
        .on_hover_text(
            RichText::new(i18n.tr("tooltip_notifications"))
                .size(palette.tooltip_text_size)
                .color(palette.tooltip_text_color),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);

    if in_progress > 0 {
        // The badge fills with the user's own accent color rather than a
        // fixed red, so it follows their chosen theme like the rest of the
        // app - the text color is then picked for contrast against
        // whatever that accent turns out to be (same luminance check used
        // for selection text in `itemviewer_preview.rs`).
        let badge_center = resp.rect.right_top() + egui::vec2(-2.0, 2.0);
        let fill_luminance = 0.299 * palette.primary.r() as f32
            + 0.587 * palette.primary.g() as f32
            + 0.114 * palette.primary.b() as f32;
        let badge_text_color = if fill_luminance > 140.0 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        ui.painter()
            .circle_filled(badge_center, 7.0, palette.primary);
        ui.painter().text(
            badge_center,
            Align2::CENTER_CENTER,
            in_progress.to_string(),
            FontId::proportional(9.0),
            badge_text_color,
        );
    }

    if resp.clicked() {
        state.panel_open = !state.panel_open;
    }

    if state.panel_open {
        let popup_pos = egui::pos2(resp.rect.right() - PANEL_WIDTH, resp.rect.bottom() + 6.0);

        let area_resp = egui::Area::new(egui::Id::new("notifications_panel_area"))
            .fixed_pos(popup_pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(PANEL_WIDTH);

                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(i18n.tr("notifications_title"))
                                .size(palette.text_size)
                                .strong()
                                .color(ui.visuals().text_color()),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if !state.operations.is_empty()
                                && eden_button(ui, palette, &i18n.tr("notifications_clear_all"))
                                    .clicked()
                            {
                                state.clear_all();
                            }
                        });
                    });

                    ui.add_space(4.0);
                    ui.separator();

                    if state.operations.is_empty() {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(i18n.tr("notifications_empty"))
                                .size(palette.text_size)
                                .color(palette.icon_color),
                        );
                        ui.add_space(8.0);
                    } else {
                        let mut to_remove = None;
                        egui::ScrollArea::vertical()
                            .max_height(PANEL_MAX_HEIGHT)
                            .show(ui, |ui| {
                                for op in &state.operations {
                                    draw_operation_row(ui, i18n, palette, op, &mut to_remove);
                                }
                            });
                        if let Some(id) = to_remove {
                            state.operations.retain(|o| o.id != id);
                        }
                    }
                });
            })
            .response;

        // `resp.clicked()` is excluded here because the Area is drawn (and
        // its `clicked_elsewhere` computed) in the very same frame the bell
        // is clicked to open it - without this guard, that opening click
        // would register as "elsewhere" (it landed on the bell icon, not
        // yet inside the Area) and close the panel on the same frame it
        // opened.
        if area_resp.clicked_elsewhere() && !resp.clicked() {
            state.panel_open = false;
        }
    }
}

fn draw_operation_row(
    ui: &mut egui::Ui,
    i18n: &I18n,
    palette: &ThemePalette,
    op: &FileOperation,
    to_remove: &mut Option<u64>,
) {
    ui.horizontal(|ui| {
        ui.painter().circle_filled(
            ui.cursor().min + egui::vec2(8.0, 10.0),
            10.0,
            status_color(op.status, palette).gamma_multiply(0.18),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(kind_icon(op.kind))
                .font(FontId::default())
                .color(status_color(op.status, palette)),
        );

        ui.vertical(|ui| {
            ui.set_width(PANEL_WIDTH - 90.0);
            ui.label(
                RichText::new(operation_title(i18n, op))
                    .size(palette.text_size)
                    .color(ui.visuals().text_color()),
            );
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(status_icon(op.status))
                        .font(FontId::proportional(11.0))
                        .color(status_color(op.status, palette)),
                );
                ui.label(
                    RichText::new(status_label(i18n, op.status))
                        .size(palette.tooltip_text_size)
                        .color(palette.icon_color),
                );
            });
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // Dismissing an in-progress row only removes it from the panel -
            // it doesn't cancel the underlying operation (there's no cancel
            // path yet, see `NotificationsState::clear_all`'s doc comment).
            if clickable_icon(ui, regular::X, palette)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                *to_remove = Some(op.id);
            }
        });
    });
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(2.0);
}

/// Draws the transient completion toast (if any operation finished
/// recently), anchored to the bottom-right of the window. Call once per
/// frame from the top-level update loop, after the rest of the UI. Keeps
/// requesting repaints while a toast is showing so its auto-hide timer
/// fires even if the app is otherwise idle.
pub fn draw_toast(
    ctx: &egui::Context,
    i18n: &I18n,
    palette: &ThemePalette,
    state: &mut NotificationsState,
) {
    let Some(toast) = &state.active_toast else {
        return;
    };

    let elapsed = toast.shown_at.elapsed();
    if elapsed >= TOAST_DURATION {
        state.active_toast = None;
        return;
    }

    let Some(op) = state.operations.iter().find(|o| o.id == toast.op_id) else {
        state.active_toast = None;
        return;
    };

    let viewport = ctx.viewport_rect();
    let toast_width = 300.0;
    let margin = 16.0;
    let pos = egui::pos2(
        viewport.right() - toast_width - margin,
        viewport.bottom() - margin - 56.0,
    );

    // Fade out over the last half-second so it doesn't just pop out of
    // existence.
    let fade_start = TOAST_DURATION.as_secs_f32() - 0.5;
    let alpha = if elapsed.as_secs_f32() > fade_start {
        (1.0 - (elapsed.as_secs_f32() - fade_start) / 0.5).clamp(0.0, 1.0)
    } else {
        1.0
    };

    egui::Area::new(egui::Id::new("notifications_toast_area"))
        .fixed_pos(pos)
        .order(egui::Order::Tooltip)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_width(toast_width);
            let frame = egui::Frame::popup(ui.style())
                .fill(ui.visuals().extreme_bg_color.gamma_multiply(alpha.max(0.05)))
                .stroke(Stroke::new(1.0, palette.borders_default.gamma_multiply(alpha)))
                .corner_radius(CornerRadius::same(palette.medium_radius));
            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(status_icon(op.status))
                            .font(FontId::proportional(16.0))
                            .color(status_color(op.status, palette).gamma_multiply(alpha)),
                    );
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(operation_title(i18n, op))
                                .size(palette.text_size)
                                .color(ui.visuals().text_color().gamma_multiply(alpha)),
                        );
                        ui.label(
                            RichText::new(status_label(i18n, op.status))
                                .size(palette.tooltip_text_size)
                                .color(palette.icon_color.gamma_multiply(alpha)),
                        );
                    });
                });
            });
        });

    ctx.request_repaint_after(Duration::from_millis(200));
}
