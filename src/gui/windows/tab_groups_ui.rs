//! Settings UI for user-defined tab groups (see `core::tab_groups`) - named
//! sets of folder paths that can be opened all at once as tabs. The same
//! path may be added to a group more than once on purpose (opening the
//! group then opens that many separate tabs for it).
//!
//! Master-detail layout: the left column lists every group (name + folder
//! count, reorderable, deletable, click to select); the right column shows
//! the selected group's own name field and its folder list. Selection is
//! tracked by the group's stable `id` (`SettingsWindow::selected_tab_group_id`)
//! rather than a positional index, so reordering or deleting an unrelated
//! group never silently re-points the detail pane at the wrong one.

use crate::core::tab_groups::{TabGroup, next_group_id};
use crate::core::utils::widgets::{apply_eden_visual_overrides, eden_button};
use crate::gui::i18n::I18n;
use crate::gui::icons::IconCache;
use crate::gui::theme::ThemePalette;
use crate::gui::windows::enums::SettingsAction;
use crate::gui::windows::settings::{
    empty_state_hint, master_detail_column_size, no_selection_hint, reorder_buttons,
    setting_label, setting_row, settings_section,
};
use crate::gui::windows::structs::SettingsWindow;
use eframe::egui;
use egui_phosphor::regular;

/// Row/header icon size for a folder's real shell icon - small enough to sit
/// inline with text like any other icon in this app's menus/lists.
const FOLDER_ICON_SIZE: egui::Vec2 = egui::vec2(16.0, 16.0);

pub fn draw_tab_groups_settings(
    ui: &mut egui::Ui,
    i18n: &I18n,
    settings: &mut SettingsWindow,
    palette: &ThemePalette,
    icon_cache: &IconCache,
) -> Option<SettingsAction> {
    let mut action = None;

    setting_label(
        ui,
        &i18n.tr("settings_tab_groups"),
        Some((&i18n.tr("tooltip_settings_tab_groups"), palette)),
        palette,
    );
    ui.add_space(8.0);

    if eden_button(
        ui,
        palette,
        &format!("{} {}", regular::PLUS, i18n.tr("tab_group_add")),
    )
    .clicked()
    {
        let id = next_group_id(&settings.current_settings.tab_groups);
        settings.current_settings.tab_groups.push(TabGroup::new(id));
        settings.selected_tab_group_id = Some(id);
        action = Some(SettingsAction::ApplySettings);
    }

    ui.add_space(10.0);

    if settings.current_settings.tab_groups.is_empty() {
        empty_state_hint(
            ui,
            palette,
            regular::FOLDERS,
            &i18n.tr("tab_group_empty_state"),
        );
        return action;
    }

    // Keep a valid selection: default to the first group the first time
    // there's anything to select, and drop a selection that pointed at a
    // group which no longer exists (deleted from another session's import,
    // or just deleted this frame below).
    if settings.selected_tab_group_id.is_none()
        || !settings
            .current_settings
            .tab_groups
            .iter()
            .any(|g| Some(g.id) == settings.selected_tab_group_id)
    {
        settings.selected_tab_group_id = settings.current_settings.tab_groups.first().map(|g| g.id);
    }

    let mut remove_index: Option<usize> = None;
    let mut move_indices: Option<(usize, usize)> = None;
    let mut changed = false;
    let total_len = settings.current_settings.tab_groups.len();

    let (col_w, col_h) = master_detail_column_size(ui);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(col_w, col_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
            egui::ScrollArea::vertical()
                .id_salt("tab_groups_list_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (index, group) in
                        settings.current_settings.tab_groups.iter().enumerate()
                    {
                        let label = if group.name.is_empty() {
                            i18n.tr("tab_group_untitled")
                        } else {
                            group.name.clone()
                        };
                        let icon = group.paths.first().and_then(|p| icon_cache.get(p, true));
                        let is_selected = Some(group.id) == settings.selected_tab_group_id;

                        let row = list_row(ui, palette, is_selected, |ui| {
                            if let Some(texture) = &icon {
                                ui.add(egui::Image::new(texture).fit_to_exact_size(FOLDER_ICON_SIZE));
                            } else {
                                ui.label(regular::FOLDERS);
                            }
                            ui.label(egui::RichText::new(&label).strong());

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if let Some(swap) = reorder_buttons(ui, palette, index, total_len) {
                                    move_indices = Some(swap);
                                }
                                if eden_button(ui, palette, regular::TRASH)
                                    .on_hover_text(i18n.tr("tab_group_remove"))
                                    .clicked()
                                {
                                    remove_index = Some(index);
                                }
                                ui.label(
                                    egui::RichText::new(format!("({})", group.paths.len()))
                                        .color(palette.tooltip_text_color),
                                );
                            });
                        });

                        if row.clicked() {
                            settings.selected_tab_group_id = Some(group.id);
                        }
                    }
                });
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(col_w, col_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
            egui::ScrollArea::vertical()
                .id_salt("tab_groups_detail_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let Some(selected_id) = settings.selected_tab_group_id else {
                        no_selection_hint(
                            ui,
                            palette,
                            regular::FOLDERS,
                            &i18n.tr("tab_group_select_hint"),
                        );
                        return;
                    };
                    let Some(group) = settings
                        .current_settings
                        .tab_groups
                        .iter_mut()
                        .find(|g| g.id == selected_id)
                    else {
                        return;
                    };

                    settings_section(ui, palette, |ui| {
                        setting_row(
                            ui,
                            |ui| {
                                setting_label(ui, &i18n.tr("tab_group_name"), None, palette);
                            },
                            |ui| {
                                apply_eden_visual_overrides(ui, palette);
                                changed |= ui
                                    .add_sized(
                                        [280.0, ui.spacing().interact_size.y],
                                        egui::TextEdit::singleline(&mut group.name),
                                    )
                                    .changed();
                            },
                        );

                        ui.add_space(8.0);
                        if eden_button(
                            ui,
                            palette,
                            &format!(
                                "{} {}",
                                regular::FOLDER_OPEN,
                                i18n.tr("tab_group_add_folder")
                            ),
                        )
                        .clicked()
                        {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                // Duplicates are allowed on purpose - the same
                                // folder can be added more than once so opening
                                // the group opens it as multiple separate tabs.
                                group.paths.push(path);
                                changed = true;
                            }
                        }

                        ui.add_space(8.0);
                        if group.paths.is_empty() {
                            ui.weak(i18n.tr("tab_group_empty"));
                        } else {
                            let mut remove_path: Option<usize> = None;
                            let mut move_path: Option<(usize, usize)> = None;
                            let path_total = group.paths.len();
                            for (path_index, path) in group.paths.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    if let Some(swap) =
                                        reorder_buttons(ui, palette, path_index, path_total)
                                    {
                                        move_path = Some(swap);
                                    }
                                    if eden_button(ui, palette, regular::TRASH).clicked() {
                                        remove_path = Some(path_index);
                                    }
                                    if let Some(texture) = icon_cache.get(path, true) {
                                        ui.add(
                                            egui::Image::new(&texture)
                                                .fit_to_exact_size(FOLDER_ICON_SIZE),
                                        );
                                    }
                                    ui.label(path.display().to_string())
                                        .on_hover_text(path.display().to_string());
                                });
                                ui.add_space(6.0);
                            }
                            if let Some((from, to)) = move_path {
                                group.paths.swap(from, to);
                                changed = true;
                            }
                            if let Some(i) = remove_path {
                                group.paths.remove(i);
                                changed = true;
                            }
                        }
                    });
                });
            },
        );
    });

    if let Some((from, to)) = move_indices {
        settings.current_settings.tab_groups.swap(from, to);
        changed = true;
    }
    if let Some(i) = remove_index {
        let removed_id = settings.current_settings.tab_groups[i].id;
        settings.current_settings.tab_groups.remove(i);
        if settings.selected_tab_group_id == Some(removed_id) {
            settings.selected_tab_group_id =
                settings.current_settings.tab_groups.first().map(|g| g.id);
        }
        changed = true;
    }

    if changed {
        action = Some(SettingsAction::ApplySettings);
    }

    action
}

/// One selectable row in a master-detail page's left-column list - a
/// smaller nested card, like `entry_card`, but click-sensitive and with a
/// distinct fill/border when selected. Returns the row's own click response
/// so the caller decides what selecting it means.
///
/// Senses the row's own background click *before* drawing `add_contents`
/// (its buttons included), rather than after - egui gives click priority to
/// whichever overlapping sense was registered later, so sensing the
/// background first and drawing (and thus sensing) the buttons afterward is
/// what lets a reorder/delete button inside the row still work, instead of
/// the row-select swallowing every click in its rect including the ones
/// meant for a button on top of it. Mirrors the same background-sensed-
/// first-then-content-drawn-on-top pattern the tab strip's own drag
/// background already uses (`mainwindow.rs`).
fn list_row(
    ui: &mut egui::Ui,
    palette: &ThemePalette,
    selected: bool,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
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
        ui.painter()
            .rect_stroke(rect, corner, egui::Stroke::new(1.5, stroke_color), egui::StrokeKind::Inside);
    }

    let content_rect = rect.shrink2(egui::vec2(10.0, 6.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(content_rect), |ui| {
        ui.horizontal_centered(|ui| add_contents(ui));
    });

    ui.add_space(6.0);

    response
}
