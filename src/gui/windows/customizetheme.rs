use crate::core::utils::fonts::get_font_list;
use crate::gui::i18n::I18n;
use crate::gui::theme::{
    PALETTE_PRESETS, ThemeMode, ThemePalette, apply_font_to_context, apply_theme_preset,
    get_default_palette, regenerate_base_derived_colors,
};
use crate::gui::utils::rgba_color_edit_button;
use crate::gui::utils::widgets::{
    apply_eden_visual_overrides, draw_dropdown, eden_button, eden_text_label, eden_toggle_button,
};
use crate::gui::windows::enums::ThemeCustomizerAction;
use crate::gui::windows::structs::ThemeCustomizer;
use eframe::egui;

/// Named accent-color presets - a quick way to retint the app (primary +
/// its derived hover/active/border shades) without hand-picking RGB values,
/// the same convenience Files Community's "color scheme" swatch picker
/// offers. Selecting one only changes `primary`; every other color a user
/// has customized is left alone.
const COLOR_SCHEME_PRESETS: &[(&str, egui::Color32)] = &[
    ("theme_scheme_blue", egui::Color32::from_rgb(0, 120, 215)),
    ("theme_scheme_teal", egui::Color32::from_rgb(0, 130, 114)),
    ("theme_scheme_green", egui::Color32::from_rgb(16, 137, 62)),
    (
        "theme_scheme_yellow_gold",
        egui::Color32::from_rgb(218, 165, 32),
    ),
    ("theme_scheme_orange", egui::Color32::from_rgb(202, 80, 16)),
    ("theme_scheme_red", egui::Color32::from_rgb(196, 43, 28)),
    ("theme_scheme_pink", egui::Color32::from_rgb(196, 69, 105)),
    ("theme_scheme_magenta", egui::Color32::from_rgb(191, 0, 119)),
    ("theme_scheme_purple", egui::Color32::from_rgb(136, 23, 152)),
    ("theme_scheme_cyan", egui::Color32::from_rgb(0, 153, 188)),
    ("theme_scheme_gray", egui::Color32::from_rgb(93, 90, 88)),
];

fn selectable_mode(
    ui: &mut egui::Ui,
    palette: &ThemePalette,
    current: ThemeMode,
    target: ThemeMode,
    label: &str,
) -> bool {
    eden_toggle_button(ui, palette, current == target, label).clicked()
}

/// Draws the theme editor's fields (typography, color-scheme swatches, core
/// colors, export/import/reset) directly into `ui` - embedded as the
/// Settings page's Appearance category rather than a floating window.
pub fn draw_theme_customizer_content(
    ui: &mut egui::Ui,
    i18n: &I18n,
    ctx: &egui::Context,
    customizer: &mut ThemeCustomizer,
    palette: &ThemePalette,
) -> Option<ThemeCustomizerAction> {
    let mut action = None;

    {
        {
            ui.set_width(ui.available_width());
            // TOP SECTION: select which palette to edit
            ui.horizontal(|ui| {
                if selectable_mode(
                    ui,
                    palette,
                    customizer.selected_mode,
                    ThemeMode::Dark,
                    &i18n.tr("theme_dark"),
                ) {
                    customizer.selected_mode = ThemeMode::Dark;
                }

                if selectable_mode(
                    ui,
                    palette,
                    customizer.selected_mode,
                    ThemeMode::Light,
                    &i18n.tr("theme_light"),
                ) {
                    customizer.selected_mode = ThemeMode::Light;
                }
            });

            let editing_palette = match customizer.selected_mode {
                ThemeMode::Dark => &mut customizer.dark_palette,
                ThemeMode::Light => &mut customizer.light_palette,
            };

            let mut changed = false;
            let mut sidebar_width_changed = false;

            // SCROLLABLE CONTENT
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.group(|ui| {
                        apply_eden_visual_overrides(ui, palette);
                        eden_text_label(ui, palette, &i18n.tr("theme_typography"));

                        ui.add_space(6.0);
                        egui::Grid::new("typography_settings")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                eden_text_label(ui, palette, &i18n.tr("theme_textsize"));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        apply_eden_visual_overrides(ui, palette);
                                        changed |= ui
                                            .add_sized(
                                                egui::vec2(90.0, 0.0),
                                                egui::DragValue::new(
                                                    &mut editing_palette.text_size,
                                                )
                                                .range(8.0..=24.0)
                                                .speed(0.2),
                                            )
                                            .changed();
                                    },
                                );
                                ui.end_row();

                                eden_text_label(ui, palette, &i18n.tr("theme_tooltip_textsize"));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        apply_eden_visual_overrides(ui, palette);
                                        changed |= ui
                                            .add_sized(
                                                egui::vec2(90.0, 0.0),
                                                egui::DragValue::new(
                                                    &mut editing_palette.tooltip_text_size,
                                                )
                                                .range(8.0..=24.0)
                                                .speed(0.2),
                                            )
                                            .changed();
                                    },
                                );
                                ui.end_row();

                                eden_text_label(ui, palette, &i18n.tr("theme_explorer_rowheight"));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        apply_eden_visual_overrides(ui, palette);
                                        changed |= ui
                                            .add_sized(
                                                egui::vec2(90.0, 0.0),
                                                egui::DragValue::new(
                                                    &mut editing_palette.row_height,
                                                )
                                                .range(8.0..=32.0)
                                                .speed(0.5),
                                            )
                                            .changed();
                                    },
                                );
                                ui.end_row();

                                eden_text_label(ui, palette, &i18n.tr("theme_sidebar_iconsize"));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        apply_eden_visual_overrides(ui, palette);
                                        changed |= ui
                                            .add_sized(
                                                egui::vec2(90.0, 0.0),
                                                egui::DragValue::new(
                                                    &mut editing_palette.sidebar_icon_size,
                                                )
                                                .range(8.0..=32.0)
                                                .speed(0.2),
                                            )
                                            .changed();
                                    },
                                );
                                ui.end_row();

                                eden_text_label(ui, palette, &i18n.tr("theme_tab_iconsize"));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        apply_eden_visual_overrides(ui, palette);
                                        changed |= ui
                                            .add_sized(
                                                egui::vec2(90.0, 0.0),
                                                egui::DragValue::new(
                                                    &mut editing_palette.tab_icon_size,
                                                )
                                                .range(8.0..=32.0)
                                                .speed(0.2),
                                            )
                                            .changed();
                                    },
                                );
                                ui.end_row();

                                eden_text_label(
                                    ui,
                                    palette,
                                    &i18n.tr("theme_sidebar_item_spacing_y"),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        apply_eden_visual_overrides(ui, palette);
                                        changed |= ui
                                            .add_sized(
                                                egui::vec2(90.0, 0.0),
                                                egui::DragValue::new(
                                                    &mut editing_palette.sidebar_item_spacing_y,
                                                )
                                                .range(-0.3..=2.0)
                                                .speed(0.1),
                                            )
                                            .changed();
                                    },
                                );
                                ui.end_row();
                            });

                        ui.add_space(8.0);

                        egui::Grid::new("typography_font_settings")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                eden_text_label(ui, palette, &i18n.tr("theme_font"));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if let Some(new_font) = font_selector(
                                            ui,
                                            palette,
                                            "theme_font_selector",
                                            &editing_palette.font_name,
                                        ) {
                                            editing_palette.font_name = new_font;
                                            apply_font_to_context(ctx, &editing_palette);
                                            changed = true;
                                        }
                                    },
                                );
                                ui.end_row();

                                eden_text_label(ui, palette, &i18n.tr("theme_mono_font"));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if let Some(new_font) = font_selector(
                                            ui,
                                            palette,
                                            "theme_mono_font_selector",
                                            &editing_palette.mono_font_name,
                                        ) {
                                            editing_palette.mono_font_name = new_font;
                                            apply_font_to_context(ctx, &editing_palette);
                                            changed = true;
                                        }
                                    },
                                );
                                ui.end_row();
                            });

                        ui.add_space(6.0);
                        ui.separator();

                        eden_text_label(ui, palette, &i18n.tr("theme_layout"));
                        ui.add_space(6.0);

                        eden_text_label(ui, palette, &i18n.tr("theme_layout_density"));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            // (label key, row_height, sidebar_item_spacing_y) - pure
                            // convenience presets over the two existing fields above;
                            // no new schema needed since both fields already exist.
                            const DENSITY_PRESETS: &[(&str, f32, f32)] = &[
                                ("theme_density_compact", 14.0, 0.2),
                                ("theme_density_comfortable", 16.0, 0.5),
                                ("theme_density_spacious", 20.0, 0.8),
                            ];
                            for &(label_key, row_height, spacing_y) in DENSITY_PRESETS {
                                let is_selected = (editing_palette.row_height - row_height).abs()
                                    < 0.01
                                    && (editing_palette.sidebar_item_spacing_y - spacing_y).abs()
                                        < 0.01;
                                if eden_toggle_button(ui, palette, is_selected, &i18n.tr(label_key))
                                    .clicked()
                                {
                                    editing_palette.row_height = row_height;
                                    editing_palette.sidebar_item_spacing_y = spacing_y;
                                    changed = true;
                                }
                            }
                        });

                        ui.add_space(8.0);

                        egui::Grid::new("theme_layout_grid")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                eden_text_label(ui, palette, &i18n.tr("theme_sidebar_width"));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        apply_eden_visual_overrides(ui, palette);
                                        let resp = ui.add_sized(
                                            egui::vec2(90.0, 0.0),
                                            egui::DragValue::new(&mut customizer.sidebar_width)
                                                .range(140.0..=600.0)
                                                .speed(1.0),
                                        );
                                        if resp.changed() {
                                            sidebar_width_changed = true;
                                        }
                                    },
                                );
                                ui.end_row();
                            });

                        ui.add_space(6.0);
                        ui.separator();

                        eden_text_label(ui, palette, &i18n.tr("theme_prebuilt_themes"));
                        ui.add_space(6.0);

                        ui.horizontal_wrapped(|ui| {
                            for preset in PALETTE_PRESETS {
                                let is_selected = editing_palette.primary == preset.accent
                                    && editing_palette.secondary_accent == preset.secondary;

                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(64.0, 32.0),
                                    egui::Sense::click(),
                                );
                                if ui.is_rect_visible(rect) {
                                    let radius = palette.small_radius;
                                    let painter = ui.painter();
                                    painter.rect_filled(
                                        rect,
                                        egui::CornerRadius::same(radius),
                                        preset.accent,
                                    );
                                    let strip_w = rect.width() / 2.0;
                                    let strip = egui::Rect::from_min_size(
                                        egui::pos2(rect.min.x + strip_w, rect.min.y),
                                        egui::vec2(strip_w, rect.height()),
                                    );
                                    painter.rect_filled(
                                        strip,
                                        egui::CornerRadius {
                                            nw: 0,
                                            sw: 0,
                                            ne: radius,
                                            se: radius,
                                        },
                                        preset.secondary,
                                    );
                                    painter.rect_stroke(
                                        rect,
                                        egui::CornerRadius::same(radius),
                                        if is_selected {
                                            egui::Stroke::new(2.0, palette.text_normal)
                                        } else {
                                            egui::Stroke::new(1.0, palette.borders_default)
                                        },
                                        egui::StrokeKind::Outside,
                                    );
                                }
                                let resp = resp.on_hover_text(i18n.tr(preset.name_key));
                                if resp.clicked() {
                                    apply_theme_preset(
                                        editing_palette,
                                        preset,
                                        customizer.selected_mode == ThemeMode::Dark,
                                    );
                                    changed = true;
                                }
                            }
                        });

                        ui.add_space(10.0);
                        ui.separator();

                        eden_text_label(ui, palette, &i18n.tr("theme_color_scheme"));
                        ui.add_space(6.0);

                        ui.horizontal_wrapped(|ui| {
                            for &(name_key, color) in COLOR_SCHEME_PRESETS {
                                let is_selected = editing_palette.primary == color;
                                let resp = ui.add(
                                    egui::Button::new("")
                                        .fill(color)
                                        .stroke(if is_selected {
                                            egui::Stroke::new(2.0, palette.text_normal)
                                        } else {
                                            egui::Stroke::new(1.0, palette.borders_default)
                                        })
                                        .corner_radius(egui::CornerRadius::same(
                                            palette.small_radius,
                                        ))
                                        .min_size(egui::vec2(26.0, 26.0)),
                                );
                                let resp = resp.on_hover_text(i18n.tr(name_key));
                                if resp.clicked() {
                                    editing_palette.primary = color;
                                    regenerate_base_derived_colors(
                                        editing_palette,
                                        customizer.selected_mode == ThemeMode::Dark,
                                    );
                                    changed = true;
                                }
                            }
                        });

                        ui.add_space(10.0);
                        ui.separator();

                        eden_text_label(ui, palette, &i18n.tr("theme_core_colors"));

                        ui.add_space(6.0);

                        egui::Grid::new("theme_corecolors")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                eden_text_label(ui, palette, &i18n.tr("theme_colors_primary"));

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        apply_eden_visual_overrides(ui, palette);

                                        let primary_changed =
                                            color_picker_control(ui, &mut editing_palette.primary);

                                        if primary_changed {
                                            regenerate_base_derived_colors(
                                                editing_palette,
                                                customizer.selected_mode == ThemeMode::Dark,
                                            );
                                            changed = true;
                                        }
                                    },
                                );
                                ui.end_row();

                                eden_text_label(
                                    ui,
                                    palette,
                                    &i18n.tr("theme_colors_primary_hover"),
                                );

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        apply_eden_visual_overrides(ui, palette);

                                        changed |= color_picker_control(
                                            ui,
                                            &mut editing_palette.primary_hover,
                                        );
                                    },
                                );
                                ui.end_row();

                                eden_text_label(
                                    ui,
                                    palette,
                                    &i18n.tr("theme_colors_primary_active"),
                                );

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        apply_eden_visual_overrides(ui, palette);

                                        changed |= color_picker_control(
                                            ui,
                                            &mut editing_palette.primary_active,
                                        );
                                    },
                                );
                                ui.end_row();

                                eden_text_label(
                                    ui,
                                    palette,
                                    &i18n.tr("theme_colors_borders_default"),
                                );

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        apply_eden_visual_overrides(ui, palette);

                                        changed |= color_picker_control(
                                            ui,
                                            &mut editing_palette.borders_default,
                                        );
                                    },
                                );
                                ui.end_row();

                                eden_text_label(
                                    ui,
                                    palette,
                                    &i18n.tr("theme_colors_borders_active"),
                                );

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        apply_eden_visual_overrides(ui, palette);

                                        changed |= color_picker_control(
                                            ui,
                                            &mut editing_palette.borders_active,
                                        );
                                    },
                                );
                                ui.end_row();

                                eden_text_label(
                                    ui,
                                    palette,
                                    &i18n.tr("theme_colors_application_background"),
                                );

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        apply_eden_visual_overrides(ui, palette);

                                        changed |= color_picker_control(
                                            ui,
                                            &mut editing_palette.application_bg_color,
                                        );
                                    },
                                );
                                ui.end_row();
                            });

                        ui.add_space(10.0);
                        ui.separator();

                        eden_text_label(ui, palette, &i18n.tr("theme_colors_text_rows"));
                        ui.add_space(6.0);
                        egui::Grid::new("theme_text_rows")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_text_normal",
                                    &mut editing_palette.text_normal,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_text_header_section",
                                    &mut editing_palette.text_header_section,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_row_selected_bg",
                                    &mut editing_palette.row_selected_bg,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_row_bg",
                                    &mut editing_palette.row_bg,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_item_viewer_row_text_selected",
                                    &mut editing_palette.item_viewer_row_text_selected,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_tooltip_text_color",
                                    &mut editing_palette.tooltip_text_color,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_tab_text_selected",
                                    &mut editing_palette.tab_text_selected,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_widget_text_inactive",
                                    &mut editing_palette.widget_text_inactive,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_widget_text_hovered",
                                    &mut editing_palette.widget_text_hovered,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_widget_text_active",
                                    &mut editing_palette.widget_text_active,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_selection_text_color",
                                    &mut editing_palette.selection_text_color,
                                );
                            });

                        ui.add_space(10.0);
                        ui.separator();

                        eden_text_label(ui, palette, &i18n.tr("theme_colors_panels_icons"));
                        ui.add_space(6.0);
                        egui::Grid::new("theme_panels_icons")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_faint_bg",
                                    &mut editing_palette.faint_bg_color,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_input_field_bg",
                                    &mut editing_palette.input_field_bg,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_sidebar_bg",
                                    &mut editing_palette.sidebar_bg_color,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_tab_inactive_bg",
                                    &mut editing_palette.tab_inactive_bg_color,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_modal_background",
                                    &mut editing_palette.modal_background_effect_color,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_icon_color",
                                    &mut editing_palette.icon_color,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_icon_windows",
                                    &mut editing_palette.icon_windows,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_icon_colored_hover",
                                    &mut editing_palette.icon_colored_hover,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_toolbar_icon_color",
                                    &mut editing_palette.toolbar_icon_color,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_resize_handle",
                                    &mut editing_palette.resize_handle,
                                );
                            });

                        ui.add_space(10.0);
                        ui.separator();

                        eden_text_label(ui, palette, &i18n.tr("theme_colors_tabs"));
                        ui.add_space(6.0);
                        egui::Grid::new("theme_tabs")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_tab_close_hover",
                                    &mut editing_palette.tab_close_hover,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_tab_close_active",
                                    &mut editing_palette.tab_close_active,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_tab_close_normal",
                                    &mut editing_palette.tab_close_normal,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_tab_add_hover",
                                    &mut editing_palette.tab_add_hover,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_pinned_tab",
                                    &mut editing_palette.pinned_tab_color,
                                );
                            });

                        ui.add_space(10.0);
                        ui.separator();

                        eden_text_label(ui, palette, &i18n.tr("theme_colors_drive_usage"));
                        ui.add_space(6.0);
                        egui::Grid::new("theme_drive_usage")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_drive_usage_critical",
                                    &mut editing_palette.drive_usage_critical,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_drive_usage_warning",
                                    &mut editing_palette.drive_usage_warning,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_drive_usage_normal",
                                    &mut editing_palette.drive_usage_normal,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_drive_usage_background",
                                    &mut editing_palette.drive_usage_background,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_drive_usage_text",
                                    &mut editing_palette.drive_usage_text,
                                );
                            });

                        ui.add_space(10.0);
                        ui.separator();

                        eden_text_label(ui, palette, &i18n.tr("theme_colors_checkboxes_buttons"));
                        ui.add_space(6.0);
                        egui::Grid::new("theme_checkboxes_buttons")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_checkbox_bg_default",
                                    &mut editing_palette.checkbox_bg_default,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_checkbox_checkmark",
                                    &mut editing_palette.checkbox_checkmark_color,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_checkbox_bg_hover",
                                    &mut editing_palette.checkbox_bg_hover,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_checkbox_bg_active",
                                    &mut editing_palette.checkbox_bg_active,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_button_background",
                                    &mut editing_palette.button_background,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_button_stroke",
                                    &mut editing_palette.button_stroke,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_button_favorite_fill",
                                    &mut editing_palette.button_favorite_fill,
                                );
                                changed |= color_row(
                                    ui,
                                    palette,
                                    i18n,
                                    "theme_colors_button_separator_handle",
                                    &mut editing_palette.button_seperator_handle_fill,
                                );
                            });

                        ui.add_space(10.0);
                        ui.separator();

                        eden_text_label(ui, palette, &i18n.tr("theme_colors_corner_radius"));
                        ui.add_space(6.0);
                        egui::Grid::new("theme_corner_radius")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                for (label_key, radius) in [
                                    ("theme_radius_small", &mut editing_palette.small_radius),
                                    ("theme_radius_medium", &mut editing_palette.medium_radius),
                                    ("theme_radius_large", &mut editing_palette.large_radius),
                                ] {
                                    eden_text_label(ui, palette, &i18n.tr(label_key));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            apply_eden_visual_overrides(ui, palette);
                                            let mut value = *radius as f32;
                                            let resp = ui.add_sized(
                                                egui::vec2(90.0, 0.0),
                                                egui::DragValue::new(&mut value)
                                                    .range(0.0..=20.0)
                                                    .speed(0.2),
                                            );
                                            if resp.changed() {
                                                *radius = value.round() as u8;
                                                changed = true;
                                            }
                                        },
                                    );
                                    ui.end_row();
                                }
                            });
                    });
                });

            // FOOTER
            ui.horizontal(|ui| {
                if eden_button(ui, palette, &i18n.tr("theme_export")).clicked() {
                    action = Some(ThemeCustomizerAction::ExportTheme(customizer.selected_mode));
                }

                if eden_button(ui, palette, &i18n.tr("theme_import")).clicked() {
                    action = Some(ThemeCustomizerAction::ImportTheme(customizer.selected_mode));
                }
                if eden_button(ui, palette, &i18n.tr("theme_reset")).clicked() {
                    let default = get_default_palette(customizer.selected_mode);
                    match customizer.selected_mode {
                        ThemeMode::Dark => customizer.dark_palette = default,
                        ThemeMode::Light => customizer.light_palette = default,
                    }
                    action = Some(ThemeCustomizerAction::ResetToDefaults(
                        customizer.selected_mode,
                    ));
                }
            });

            if changed && action.is_none() {
                action = Some(ThemeCustomizerAction::ThemeUpdated(
                    customizer.selected_mode,
                ));
            }

            if sidebar_width_changed {
                action = Some(ThemeCustomizerAction::SidebarWidthChanged(
                    customizer.sidebar_width,
                ));
            }
        }
    }

    action
}

fn color_picker_control(ui: &mut egui::Ui, color: &mut egui::Color32) -> bool {
    rgba_color_edit_button(ui, color).changed()
}

/// One label + right-aligned color-picker row inside an `egui::Grid`,
/// followed by `ui.end_row()` - the exact shape every "Core Colors" row
/// already hand-wrote, pulled out into a helper since the granular-override
/// sections below repeat it several dozen times.
fn color_row(
    ui: &mut egui::Ui,
    palette: &ThemePalette,
    i18n: &I18n,
    label_key: &str,
    color: &mut egui::Color32,
) -> bool {
    eden_text_label(ui, palette, &i18n.tr(label_key));
    let mut changed = false;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        apply_eden_visual_overrides(ui, palette);
        changed = color_picker_control(ui, color);
    });
    ui.end_row();
    changed
}

fn font_selector(
    ui: &mut egui::Ui,
    palette: &ThemePalette,
    label: &str,
    current_font: &str,
) -> Option<String> {
    let fonts = get_font_list();

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let mut selected_text = current_font.to_string();
        let mut changed = false;

        draw_dropdown(
            ui,
            palette,
            egui::Id::new(label),
            200.0,
            selected_text.clone(),
            |ui| {
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for font in fonts.iter() {
                            let rich_text = egui::RichText::new(font.as_str());

                            if ui
                                .selectable_label(font == current_font, rich_text)
                                .clicked()
                            {
                                selected_text = font.clone();
                                changed = true;
                                ui.close();
                            }
                        }
                    });
            },
        );

        if changed { Some(selected_text) } else { None }
    })
    .inner
}
