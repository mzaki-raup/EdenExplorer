//! Settings > Sidebar: one row per section, showing and ordering it in the
//! same place.
//!
//! Visibility used to live in General ("Sidebar Sections") and the order
//! here, which meant looking in two places to arrange one list. The two
//! settings come from three different stores (`ui_prefs` for Folders/Cloud/
//! Linux, `SidebarSectionVisibility` for the rest, and nothing at all for
//! Places and Storage, which are always shown) - that is a storage detail,
//! not something the user should have to know.

use crate::core::sidebar_sections::{SidebarSection, ordered};
use crate::core::utils::widgets::eden_button;
use crate::gui::i18n::I18n;
use crate::gui::theme::ThemePalette;
use crate::gui::windows::enums::SettingsAction;
use crate::gui::windows::settings::{
    reorder_buttons, setting_checkbox, setting_label, settings_section,
};
use crate::gui::windows::structs::SettingsWindow;
use eframe::egui;
use egui_phosphor::regular;

/// The list is a column of short rows, so it gets its own width rather than
/// the page's: stretched across the whole settings pane the reorder buttons
/// end up stranded an inch and a half from the name they belong to.
const LIST_WIDTH: f32 = 420.0;

pub fn draw_sidebar_settings(
    ui: &mut egui::Ui,
    i18n: &I18n,
    settings: &mut SettingsWindow,
    palette: &ThemePalette,
) -> Option<SettingsAction> {
    let mut action = None;

    // Seeded the first time the page is opened rather than at startup: this
    // is the only thing that reads it, and it is one small file.
    if settings.sidebar_section_order.is_empty() {
        settings.sidebar_section_order = ordered(&crate::core::indexer::load_sidebar_order().order);
    }

    setting_label(
        ui,
        &i18n.tr("settings_category_sidebar"),
        Some((&i18n.tr("tooltip_settings_sidebar"), palette)),
        palette,
    );
    ui.add_space(8.0);

    let order = settings.sidebar_section_order.clone();
    let total = order.len();
    let mut swap: Option<(usize, usize)> = None;
    let mut visibility_changed = false;

    ui.set_max_width(LIST_WIDTH.min(ui.available_width()));
    settings_section(ui, palette, |ui| {
        for (index, section) in order.iter().copied().enumerate() {
            ui.horizontal(|ui| {
                match visibility_of(settings, section) {
                    Some(shown) => {
                        if setting_checkbox(
                            ui,
                            palette,
                            shown,
                            egui::RichText::new(i18n.tr(section.label_key()))
                                .color(palette.text_normal),
                            format!("sidebar_order_shown_{}", section.id()),
                        ) {
                            visibility_changed = true;
                        }
                    }
                    // Places and Storage cannot be hidden. Drawn disabled
                    // rather than left out so every row still lines up.
                    None => {
                        ui.add_enabled_ui(false, |ui| {
                            let mut always = true;
                            setting_checkbox(
                                ui,
                                palette,
                                &mut always,
                                egui::RichText::new(i18n.tr(section.label_key()))
                                    .color(palette.text_normal),
                                format!("sidebar_order_shown_{}", section.id()),
                            );
                        })
                        .response
                        .on_hover_text(i18n.tr("sidebar_order_always_shown"));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(pair) = reorder_buttons(ui, palette, index, total) {
                        swap = Some(pair);
                    }
                });
            });
            ui.add_space(6.0);
        }
    });

    if let Some((from, to)) = swap
        && from < total
        && to < total
    {
        settings.sidebar_section_order.swap(from, to);
        action = Some(SettingsAction::SaveSidebarOrder);
    } else if visibility_changed {
        action = Some(SettingsAction::ApplySettings);
    }

    ui.add_space(10.0);
    if eden_button(
        ui,
        palette,
        &format!("{} {}", regular::ARROW_COUNTER_CLOCKWISE, i18n.tr("sidebar_order_reset")),
    )
    .clicked()
    {
        settings.sidebar_section_order = SidebarSection::ALL.to_vec();
        action = Some(SettingsAction::SaveSidebarOrder);
    }

    action
}

/// Where a section's "is it shown" flag lives, or `None` for the two that
/// are always shown.
fn visibility_of(settings: &mut SettingsWindow, section: SidebarSection) -> Option<&mut bool> {
    let current = &mut settings.current_settings;
    match section {
        SidebarSection::Places | SidebarSection::Storage => None,
        SidebarSection::Folders => Some(&mut current.ui_prefs.folder_tree),
        SidebarSection::Cloud => Some(&mut current.ui_prefs.sidebar_cloud),
        SidebarSection::Linux => Some(&mut current.ui_prefs.sidebar_linux),
        SidebarSection::Favorites => Some(&mut current.sidebar_visibility.favorites),
        SidebarSection::Tags => Some(&mut current.sidebar_visibility.tags),
        SidebarSection::SavedSearches => Some(&mut current.sidebar_visibility.saved_searches),
        SidebarSection::RecentLocations => {
            Some(&mut current.sidebar_visibility.recent_locations)
        }
        SidebarSection::SharedNetwork => Some(&mut current.sidebar_visibility.shared_network),
    }
}
