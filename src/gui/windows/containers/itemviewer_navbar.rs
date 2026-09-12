use crate::core::fs::{MY_PC_PATH, MY_RECYCLE_BIN_PATH, parse_tag_view_path};
use crate::core::network;
use crate::core::portable;
use crate::gui::i18n::I18n;
use crate::gui::icons::IconCache;
use crate::gui::theme::ThemePalette;
use crate::gui::utils::{
    clear_clipboard_files, clickable_icon_sized_with_base_color, expand_environment_variables,
};
use crate::gui::windows::containers::enums::ItemViewerNavAction;
use crate::gui::windows::containers::structs::{
    Breadcrumb, ItemViewerDisplayMode, ItemViewerNavBarAction, RenderedBreadcrumb, TabView,
    TagGroup,
};
use eframe::egui;
use egui::text::{CCursor, CCursorRange};
use egui::{FontFamily, FontId};
use egui_phosphor::{fill, regular};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Font size for the back/forward/up/refresh/new-folder/star/display-mode
/// toolbar icons - a bit larger than the default so the toolbar reads clearly.
const TOOLBAR_ICON_SIZE: f32 = 18.0;
/// Icon size used inline with breadcrumb/address-bar labels (This PC, Recycle
/// Bin, Settings, and the per-segment folder icons).
const BREADCRUMB_ICON_SIZE: f32 = 15.0;
/// Extra vertical breathing room above and below each toolbar/address-bar
/// row - the caller must reserve this much extra height per row (see
/// `tabbar_height` in `explorer.rs`) or the rows get clipped/cramped.
pub const TOOLBAR_ROW_VERTICAL_PADDING: f32 = 4.0;

pub fn draw_itemviewer_navigation_bar(
    ui: &mut egui::Ui,
    i18n: &I18n,
    icon_cache: &IconCache,
    tab: &mut TabView,
    tab_id: u64,
    palette: &ThemePalette,
    is_favorited: bool,
    drag_active: bool,
    drag_hover_target: Option<PathBuf>,
    is_split_pane: bool,
    tags: &[TagGroup],
) -> ItemViewerNavBarAction {
    let mut action = ItemViewerNavBarAction::default();
    let tabbar_rect = ui.available_rect_before_wrap();
    ui.set_clip_rect(tabbar_rect);

    let pointer_pos = ui.input(|i| i.pointer.interact_pos().or_else(|| i.pointer.hover_pos()));
    let pointer_released =
        ui.input(|i| i.pointer.any_released() && i.pointer.interact_pos().is_some());
    let hovered_target_ref = drag_hover_target.as_ref();
    let mut breadcrumb_drop_target: Option<PathBuf> = None;
    let pointer_in_tabbar = pointer_pos
        .map(|pos| tabbar_rect.contains(pos))
        .unwrap_or(false);
    let can_go_back = tab.nav.can_go_back();
    let can_go_forward = tab.nav.can_go_forward();
    let is_recycle_bin = tab.nav.is_recycle_bin();
    let is_settings = tab.nav.is_settings();
    // The Settings tab has no file-management actions of its own; treat it like the
    // recycle bin for the purposes of disabling those toolbar buttons.
    let disable_file_actions = is_recycle_bin || is_settings || tab.nav.is_tag_view();

    if is_split_pane {
        // In a dual-pane split, each pane is only half-width, so the address
        // bar gets its own full-width row above the toolbar instead of
        // squeezing in beside it.
        ui.add_space(TOOLBAR_ROW_VERTICAL_PADDING);
        ui.horizontal(|ui| {
            ui.add_space(1.5);
            draw_bordered_breadcrumb(
                ui,
                i18n,
                icon_cache,
                tab,
                tab_id,
                palette,
                drag_active,
                hovered_target_ref,
                pointer_pos,
                pointer_released,
                &mut breadcrumb_drop_target,
                &mut action,
                tags,
            );
        });
        ui.add_space(TOOLBAR_ROW_VERTICAL_PADDING);

        ui.add_space(TOOLBAR_ROW_VERTICAL_PADDING);
        ui.horizontal(|ui| {
            ui.add_space(1.5);
            let toolbar_action = draw_navigation_bar_buttons(
                ui,
                i18n,
                palette,
                is_favorited,
                &tab.nav.current,
                tab.nav.is_root() || is_settings,
                disable_file_actions,
                can_go_back,
                can_go_forward,
                tab.display_mode,
            );

            merge_toolbar_action(&mut action, toolbar_action);
            if let Some(mode) = action.set_display_mode {
                tab.display_mode = mode;
            }
        });
        ui.add_space(TOOLBAR_ROW_VERTICAL_PADDING);
    } else {
        ui.add_space(TOOLBAR_ROW_VERTICAL_PADDING);
        // `horizontal_centered` (rather than plain `horizontal`) reserves the
        // full row height up front and centers everything against it, so the
        // toolbar icons and the bordered address bar - which are different
        // heights once the address bar's own frame margin is added - line up
        // on the same vertical center instead of each keying off their own
        // natural height.
        ui.horizontal_centered(|ui| {
            ui.add_space(1.5);
            let toolbar_action = draw_navigation_bar_buttons(
                ui,
                i18n,
                palette,
                is_favorited,
                &tab.nav.current,
                tab.nav.is_root() || is_settings,
                disable_file_actions,
                can_go_back,
                can_go_forward,
                tab.display_mode,
            );

            merge_toolbar_action(&mut action, toolbar_action);
            if let Some(mode) = action.set_display_mode {
                tab.display_mode = mode;
            }

            ui.separator();

            draw_bordered_breadcrumb(
                ui,
                i18n,
                icon_cache,
                tab,
                tab_id,
                palette,
                drag_active,
                hovered_target_ref,
                pointer_pos,
                pointer_released,
                &mut breadcrumb_drop_target,
                &mut action,
                tags,
            );
        });
        ui.add_space(TOOLBAR_ROW_VERTICAL_PADDING);
    }

    if action.nav.is_none() && pointer_in_tabbar && !tab.breadcrumb_path_editing {
        if can_go_back && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra1)) {
            action.nav = Some(ItemViewerNavAction::Back);
        } else if can_go_forward
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra2))
        {
            action.nav = Some(ItemViewerNavAction::Forward);
        }
    }

    action.move_files_to_breadcrumb_dir = breadcrumb_drop_target;
    action
}

/// Wraps `draw_breadcrumb_row_contents` in a bordered frame, so the address
/// bar always reads as its own distinct field - previously it only gained a
/// border once you clicked in to type a path.
#[allow(clippy::too_many_arguments)]
fn draw_bordered_breadcrumb(
    ui: &mut egui::Ui,
    i18n: &I18n,
    icon_cache: &IconCache,
    tab: &mut TabView,
    tab_id: u64,
    palette: &ThemePalette,
    drag_active: bool,
    hovered_target_ref: Option<&PathBuf>,
    pointer_pos: Option<egui::Pos2>,
    pointer_released: bool,
    breadcrumb_drop_target: &mut Option<PathBuf>,
    action: &mut ItemViewerNavBarAction,
    tags: &[TagGroup],
) {
    // Computed before the frame is created: inside a horizontal layout, a
    // frame otherwise shrinks to fit its content (like an inline element)
    // instead of stretching to fill the rest of the row, so the border
    // would only wrap tightly around the path text.
    let full_width = ui.available_width();
    let side_margin = 6.0;
    // Gap left after the field so it never sits flush against the pane's own
    // right edge/border, in both single-pane and split-pane layouts.
    let right_gap = 8.0;

    egui::Frame::NONE
        .stroke(egui::Stroke::new(1.5, palette.borders_default))
        .corner_radius(egui::CornerRadius::same(palette.small_radius))
        .inner_margin(egui::Margin {
            left: side_margin as i8,
            right: side_margin as i8,
            top: 3,
            bottom: 5,
        })
        .show(ui, |ui| {
            let target_width = (full_width - right_gap).max(0.0);
            ui.set_min_width((target_width - side_margin * 2.0).max(0.0));
            let breadcrumb_width = ui.available_width();
            draw_breadcrumb_row_contents(
                ui,
                i18n,
                icon_cache,
                tab,
                tab_id,
                palette,
                drag_active,
                hovered_target_ref,
                pointer_pos,
                pointer_released,
                breadcrumb_drop_target,
                action,
                breadcrumb_width,
                tags,
            );
        });
}

/// Replaces the breadcrumb row's contents with an inline query box + scope
/// toggle while `tab.search_box_editing` is set - entered via the navbar's
/// search icon or Ctrl+F. Enter submits (`action.open_search`), Escape
/// cancels back to the normal breadcrumb.
fn draw_search_box_contents(
    ui: &mut egui::Ui,
    i18n: &I18n,
    tab: &mut TabView,
    tab_id: u64,
    palette: &ThemePalette,
    action: &mut ItemViewerNavBarAction,
) {
    // `horizontal_wrapped` rather than a plain `horizontal`: the scope
    // buttons' width varies with the current language's translation and
    // this row's available width shrinks in a split pane, so a fixed
    // pixel budget for them can't be guaranteed to fit - wrapping to a
    // second line keeps them inside the frame's own border instead of
    // spilling past it (which a plain `horizontal` would just clip/overflow).
    ui.horizontal_wrapped(|ui| {
        // The query field gets its own bordered box (distinct from the
        // plain toolbar row it lives in) so it visually reads as "the
        // thing you type into", set apart from the This-folder/Everywhere
        // toggle buttons beside it rather than blending into the same bare
        // row. This whole row is already nested inside the breadcrumb's own
        // outer bordered frame (`draw_bordered_breadcrumb`) with a height
        // budget sized for un-boxed content - giving this inner frame its
        // own non-zero top/bottom margin on top of that stacks two frames'
        // worth of vertical padding and overflows past the row's bottom
        // border. Horizontal-only margin keeps this box's height identical
        // to the plain icon+text it replaces.
        egui::Frame::NONE
            .stroke(egui::Stroke::new(1.0, palette.borders_default))
            .corner_radius(egui::CornerRadius::same(palette.small_radius))
            .inner_margin(egui::Margin::symmetric(8, 0))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(regular::MAGNIFYING_GLASS)
                        .size(palette.text_size + 2.0)
                        .color(palette.icon_color),
                );
                ui.add_space(6.0);

                let text_edit_id = ui.id().with(("search_box_edit", tab_id));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut tab.search_box_buffer)
                        .id(text_edit_id)
                        .hint_text(i18n.tr("search_placeholder"))
                        .frame(egui::Frame::NONE)
                        .desired_width(220.0)
                        .font(FontId::new(
                            palette.text_size + 2.0,
                            egui::FontFamily::Proportional,
                        )),
                );
                resp.request_focus();
            });

        ui.add_space(8.0);

        let is_everywhere = matches!(tab.search_box_scope, crate::core::everything::SearchScope::Everywhere);
        if ui
            .selectable_label(!is_everywhere, i18n.tr("search_scope_current_folder"))
            .clicked()
            && is_everywhere
        {
            tab.search_box_scope =
                crate::core::everything::SearchScope::CurrentFolder(tab.nav.current.clone());
        }
        if ui
            .selectable_label(is_everywhere, i18n.tr("search_scope_everywhere"))
            .clicked()
            && !is_everywhere
        {
            tab.search_box_scope = crate::core::everything::SearchScope::Everywhere;
        }

        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));

        if enter && !tab.search_box_buffer.trim().is_empty() {
            action.open_search = Some((
                tab.search_box_buffer.trim().to_string(),
                tab.search_box_scope.clone(),
            ));
            tab.search_box_editing = false;
        } else if escape {
            tab.search_box_editing = false;
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_breadcrumb_row_contents(
    ui: &mut egui::Ui,
    i18n: &I18n,
    icon_cache: &IconCache,
    tab: &mut TabView,
    tab_id: u64,
    palette: &ThemePalette,
    drag_active: bool,
    hovered_target_ref: Option<&PathBuf>,
    pointer_pos: Option<egui::Pos2>,
    pointer_released: bool,
    breadcrumb_drop_target: &mut Option<PathBuf>,
    action: &mut ItemViewerNavBarAction,
    breadcrumb_width: f32,
    tags: &[TagGroup],
) {
    if tab.search_box_editing {
        draw_search_box_contents(ui, i18n, tab, tab_id, palette, action);
        return;
    }

    {
        if tab.breadcrumb_path_editing {
            let text_edit_id = ui.id().with(("breadcrumbs_path_edit", tab_id));

            if tab.breadcrumb_select_all_on_focus {
                let mut state =
                    egui::widgets::text_edit::TextEditState::load(ui.ctx(), text_edit_id)
                        .unwrap_or_default();
                let cursor_end = CCursor::new(tab.breadcrumb_path_buffer.chars().count());
                state
                    .cursor
                    .set_char_range(Some(CCursorRange::two(CCursor::new(0), cursor_end)));
                state.store(ui.ctx(), text_edit_id);
            }

            let mut offset_x = 0.0;
            if tab.breadcrumb_path_error {
                let t = (ui.input(|i| i.time) - tab.breadcrumb_path_error_animation_time) as f32;

                if t < 0.4 {
                    let frequency = 30.0_f32;
                    let amplitude = 4.0 * (1.0 - t / 0.4); // decay
                    offset_x = (t * frequency).sin() * amplitude;
                }
            }

            ui.add_space(offset_x);

            let time_since_error = ui.input(|i| i.time) - tab.breadcrumb_path_error_animation_time;
            let error_strength = (1.0 - time_since_error * 2.0).clamp(0.0, 1.0);

            let stroke_color = if tab.breadcrumb_path_error {
                egui::Color32::from_rgba_premultiplied(255, 80, 80, (255.0 * error_strength) as u8)
            } else {
                ui.visuals().widgets.inactive.bg_stroke.color
            };

            let frame = egui::Frame::NONE
                .stroke(egui::Stroke::new(1.5, stroke_color))
                .corner_radius(egui::CornerRadius::same(4));

            let mut resp = frame
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut tab.breadcrumb_path_buffer)
                            .id(text_edit_id)
                            .frame(if tab.breadcrumb_path_error {
                                egui::Frame::NONE
                            } else {
                                egui::Frame::default()
                            })
                            .desired_width(ui.available_width() - 40.0)
                            .font(FontId::new(
                                palette.text_size + 2.0,
                                egui::FontFamily::Proportional,
                            )),
                    )
                })
                .inner;

            if resp.changed() {
                tab.breadcrumb_path_error = false;
            }

            if tab.breadcrumb_path_error && resp.hovered() {
                resp = resp.on_hover_text(
                    egui::RichText::new(i18n.tr("tooltip_path_does_not_exist"))
                        .size(palette.tooltip_text_size)
                        .color(palette.tooltip_text_color),
                );
            }

            if !tab.breadcrumb_just_started_editing || tab.breadcrumb_path_error {
                resp.request_focus();
                tab.breadcrumb_just_started_editing = true;
            }

            if resp.has_focus() {
                tab.breadcrumb_select_all_on_focus = false;
            }

            action.is_breadcrumb_path_edit_active = resp.has_focus();

            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));

            let mut exit_edit_mode = false;

            if enter {
                let input = tab.breadcrumb_path_buffer.trim().trim_matches('"');
                let expanded_input = expand_environment_variables(input);
                let new_path = PathBuf::from(&expanded_input);

                // A bare network host (e.g. "\\server") can't be queried with
                // `exists()` like a real directory, so accept it on faith - the
                // share listing will simply come back empty if it's unreachable.
                let is_valid_target =
                    new_path.exists() || network::unc_host_only(&new_path).is_some();

                if is_valid_target {
                    action.nav_to = Some(new_path);
                    exit_edit_mode = true;
                } else {
                    println!(
                        "{}: {} ({}: {})",
                        i18n.tr("tooltip_invalid_path"),
                        tab.breadcrumb_path_buffer,
                        i18n.tr("tooltip_invalid_path_expanded"),
                        expanded_input
                    );
                    tab.breadcrumb_path_error = true;
                    tab.breadcrumb_path_error_animation_time = ui.input(|i| i.time);
                }
            } else if escape || resp.lost_focus() {
                tab.breadcrumb_path_buffer = tab.nav.current.to_string_lossy().to_string();
                exit_edit_mode = true;
            }

            if exit_edit_mode {
                tab.breadcrumb_path_editing = false;
                tab.breadcrumb_just_started_editing = false;
                tab.breadcrumb_path_error = false;
                tab.breadcrumb_path_error_animation_time = 0.0;

                ui.memory_mut(|mem| mem.surrender_focus(text_edit_id));
            }
        } else if tab.nav.is_root() || tab.nav.is_recycle_bin() || tab.nav.is_settings() {
            let pc_icon_path = PathBuf::from(MY_PC_PATH);
            if tab.nav.is_recycle_bin() {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(regular::TRASH)
                            .size(BREADCRUMB_ICON_SIZE)
                            .color(palette.text_header_section),
                    )
                    .selectable(false),
                );
            } else if tab.nav.is_settings() {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(regular::GEAR)
                            .size(BREADCRUMB_ICON_SIZE)
                            .color(palette.text_header_section),
                    )
                    .selectable(false),
                );
            } else if let Some(icon) = icon_cache.get(&pc_icon_path, true) {
                ui.add(
                    egui::Image::new(&icon)
                        .fit_to_exact_size(egui::vec2(BREADCRUMB_ICON_SIZE, BREADCRUMB_ICON_SIZE)),
                );
            }

            if ui
                .add(
                    egui::Label::new(
                        egui::RichText::new(if tab.nav.is_recycle_bin() {
                            i18n.tr("recycle_bin")
                        } else if tab.nav.is_settings() {
                            i18n.tr("settings")
                        } else {
                            i18n.tr("thispc")
                        })
                        .size(palette.text_size + 2.0)
                        .color(palette.text_header_section),
                    )
                    .selectable(false)
                    .sense(egui::Sense::click()),
                )
                .clicked()
                && !tab.nav.is_settings()
            {
                action.nav_to = Some(if tab.nav.is_recycle_bin() {
                    PathBuf::from(MY_RECYCLE_BIN_PATH)
                } else {
                    PathBuf::from(MY_PC_PATH)
                });
            }
        } else if let Some(group_id) = parse_tag_view_path(&tab.nav.current) {
            let group = tags.iter().find(|g| g.id == group_id);
            let color = group
                .map(|g| g.color)
                .unwrap_or(palette.text_header_section);

            ui.add(
                egui::Label::new(
                    egui::RichText::new(fill::TAG)
                        .size(BREADCRUMB_ICON_SIZE)
                        .color(color),
                )
                .selectable(false),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(group.map(|g| g.name.as_str()).unwrap_or(""))
                        .size(palette.text_size + 2.0)
                        .color(palette.text_header_section),
                )
                .selectable(false),
            );
        } else {
            let font_id =
                egui::FontId::new(palette.text_size + 2.0, egui::FontFamily::Proportional);
            let breadcrumbs = build_breadcrumbs(&tab.nav.current);
            let segments = layout_breadcrumbs(ui, &breadcrumbs, breadcrumb_width, &font_id);
            let mut first = true;
            let mut breadcrumbs_right = 0.0;

            for crumb in &segments {
                if !first {
                    let old_spacing = ui.spacing().item_spacing;
                    ui.spacing_mut().item_spacing.x = 10.0;

                    ui.label(
                        egui::RichText::new(">")
                            .size(palette.text_size + 2.0)
                            .color(palette.text_header_section),
                    );

                    ui.spacing_mut().item_spacing = old_spacing;
                }
                first = false;

                let resp = draw_breadcrumb(ui, crumb, icon_cache, palette);

                handle_breadcrumb_drag(
                    ui,
                    &resp,
                    crumb,
                    &segments,
                    palette,
                    drag_active,
                    hovered_target_ref,
                    pointer_pos,
                    pointer_released,
                    breadcrumb_drop_target,
                    action,
                );

                handle_breadcrumb_hover(ui, &resp);

                handle_breadcrumb_click(&resp, crumb, &segments, drag_active, action);

                breadcrumbs_right = resp.rect.right();
            }

            // ---- Detect click in empty area to enter path editing ----
            let available_rect = ui.available_rect_before_wrap();
            let empty_area = egui::Rect::from_min_max(
                egui::pos2(breadcrumbs_right, available_rect.top()),
                available_rect.right_bottom(),
            );

            if ui
                .interact(
                    empty_area,
                    ui.id().with("breadcrumb_empty"),
                    egui::Sense::click(),
                )
                .clicked()
            {
                tab.breadcrumb_path_editing = true;
                tab.breadcrumb_path_buffer = tab.nav.current.to_string_lossy().to_string();
            }
        }
    }
}

fn nav_icon_button(
    ui: &mut egui::Ui,
    icon: &str,
    palette: &ThemePalette,
    enabled: bool,
    hover_text: &str,
) -> egui::Response {
    let font_id = egui::FontId::proportional(TOOLBAR_ICON_SIZE);
    let base_color = if enabled {
        palette.toolbar_icon_color
    } else {
        ui.visuals().text_color()
    };
    let resp = ui.add_enabled(
        enabled,
        egui::Label::new(
            egui::RichText::new(icon)
                .font(font_id.clone())
                .color(base_color),
        )
        .selectable(false)
        .sense(egui::Sense::click()),
    );

    if enabled && resp.hovered() {
        ui.painter().text(
            resp.rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            font_id.clone(),
            palette.primary,
        );
    }

    let resp = resp.on_hover_text(
        egui::RichText::new(hover_text)
            .size(palette.tooltip_text_size)
            .color(palette.tooltip_text_color),
    );

    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

fn draw_navigation_bar_buttons(
    ui: &mut egui::Ui,
    i18n: &I18n,
    palette: &ThemePalette,
    is_favorited: bool,
    current_dir: &Path,
    is_root: bool,
    is_recycle_bin: bool,
    can_go_back: bool,
    can_go_forward: bool,
    display_mode: ItemViewerDisplayMode,
) -> ItemViewerNavBarAction {
    let mut action = ItemViewerNavBarAction::default();

    // Navigation buttons
    if nav_icon_button(
        ui,
        regular::ARROW_LEFT,
        palette,
        can_go_back,
        &i18n.tr("tooltip_nav_back"),
    )
    .clicked()
        && can_go_back
    {
        action.nav = Some(ItemViewerNavAction::Back);
    }

    if nav_icon_button(
        ui,
        regular::ARROW_RIGHT,
        palette,
        can_go_forward,
        &i18n.tr("tooltip_nav_forward"),
    )
    .clicked()
        && can_go_forward
    {
        action.nav = Some(ItemViewerNavAction::Forward);
    }

    let can_go_up = !is_root && !is_recycle_bin;
    if nav_icon_button(
        ui,
        regular::ARROW_UP,
        palette,
        can_go_up,
        &i18n.tr("tooltip_nav_up"),
    )
    .clicked()
        && can_go_up
    {
        action.nav = Some(ItemViewerNavAction::Up);
    }

    if clickable_icon_sized_with_base_color(
        ui,
        regular::ARROWS_CLOCKWISE,
        palette,
        TOOLBAR_ICON_SIZE,
        palette.toolbar_icon_color,
    )
        .on_hover_text(
            egui::RichText::new(i18n.tr("tooltip_refresh"))
                .size(palette.tooltip_text_size)
                .color(palette.tooltip_text_color),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        action.refresh_current_directory = true;
        clear_clipboard_files();
    }

    // Action buttons
    if nav_icon_button(
        ui,
        regular::FOLDER_PLUS,
        palette,
        !is_recycle_bin,
        &i18n.tr("tooltip_newfolder"),
    )
    .clicked()
        && !is_recycle_bin
    {
        action.create_folder = true;
    }

    if nav_icon_button(
        ui,
        regular::FILE_PLUS,
        palette,
        !is_recycle_bin,
        &i18n.tr("tooltip_newfile"),
    )
    .clicked()
        && !is_recycle_bin
    {
        action.create_file = true;
    }

    let star_icon = if is_favorited {
        fill::STAR
    } else {
        regular::STAR
    };
    let star_color = if is_favorited {
        palette.button_favorite_fill
    } else {
        palette.tooltip_text_color
    };

    let star_font = if is_favorited {
        FontId::new(TOOLBAR_ICON_SIZE, FontFamily::Name("phosphor_fill".into()))
    } else {
        FontId::new(TOOLBAR_ICON_SIZE, FontFamily::Proportional)
    };

    let star_resp = ui.add_enabled(
        !is_root && !is_recycle_bin,
        egui::Label::new(
            egui::RichText::new(star_icon)
                .font(star_font)
                .color(star_color),
        )
        .selectable(false)
        .sense(egui::Sense::click()),
    );

    let star_resp = if is_root || is_recycle_bin {
        star_resp.on_hover_text(
            egui::RichText::new(i18n.tr("tooltip_favorites_disabled"))
                .size(palette.tooltip_text_size)
                .color(palette.tooltip_text_color),
        )
    } else {
        star_resp
            .on_hover_text(
                egui::RichText::new(if is_favorited {
                    i18n.tr("tooltip_favorites_remove")
                } else {
                    i18n.tr("tooltip_favorites_add")
                })
                .size(palette.tooltip_text_size)
                .color(palette.tooltip_text_color),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
    };

    if star_resp.clicked() {
        if is_favorited {
            action.remove_favorite = true;
        } else {
            action.add_favorite = true;
        }
    }

    if nav_icon_button(
        ui,
        regular::MAGNIFYING_GLASS,
        palette,
        true,
        &i18n.tr("tooltip_search"),
    )
    .clicked()
    {
        action.activate_search_box = true;
    }

    let display_mode_enabled = !is_recycle_bin && !is_root;
    for (mode, icon, tooltip_key) in [
        (
            ItemViewerDisplayMode::Details,
            regular::ROWS,
            "view_details",
        ),
        (
            ItemViewerDisplayMode::Gallery,
            regular::IMAGES_SQUARE,
            "view_gallery",
        ),
        (
            ItemViewerDisplayMode::Columns,
            regular::COLUMNS,
            "view_columns",
        ),
        (
            ItemViewerDisplayMode::ColumnPreview,
            regular::COLUMNS_PLUS_RIGHT,
            "view_column_preview",
        ),
        (ItemViewerDisplayMode::Preview, regular::EYE, "view_preview"),
        (
            ItemViewerDisplayMode::DetailPreview,
            regular::SIDEBAR,
            "view_detail_preview",
        ),
    ] {
        let color = if display_mode == mode {
            palette.primary
        } else {
            ui.visuals().text_color()
        };

        if ui
            .add_enabled(
                display_mode_enabled,
                egui::Label::new(
                    egui::RichText::new(icon)
                        .size(TOOLBAR_ICON_SIZE)
                        .color(color),
                )
                .selectable(false)
                .sense(egui::Sense::click()),
            )
            .on_hover_text(
                egui::RichText::new(i18n.tr(tooltip_key))
                    .size(palette.tooltip_text_size)
                    .color(palette.tooltip_text_color),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
            && display_mode_enabled
        {
            action.set_display_mode = Some(mode);
        }
    }

    if nav_icon_button(
        ui,
        regular::TERMINAL,
        palette,
        !is_recycle_bin,
        &i18n.tr("tooltip_open_terminal"),
    )
    .clicked()
        && !is_recycle_bin
    {
        open_default_terminal(current_dir);
    }

    action
}

pub(crate) fn open_default_terminal(current_dir: &Path) {
    let start_dir = if current_dir.to_string_lossy() == MY_PC_PATH || !current_dir.exists() {
        dirs::home_dir().unwrap_or_else(|| current_dir.to_path_buf())
    } else {
        current_dir.to_path_buf()
    };

    let launched = Command::new("wt.exe")
        .arg("-d")
        .arg(&start_dir)
        .spawn()
        .is_ok()
        || Command::new("powershell.exe")
            .current_dir(&start_dir)
            .spawn()
            .is_ok()
        || Command::new("cmd.exe")
            .current_dir(&start_dir)
            .spawn()
            .is_ok();

    if !launched {
        eprintln!("Failed to open terminal");
    }
}

fn measure_breadcrumb_width(ui: &egui::Ui, font_id: &egui::FontId, text: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font_id.clone(), ui.visuals().text_color())
        .size()
        .x
}

fn truncate_breadcrumb_to_width(
    ui: &egui::Ui,
    font_id: &egui::FontId,
    text: &str,
    max_width: f32,
) -> (String, bool) {
    if measure_breadcrumb_width(ui, font_id, text) <= max_width {
        return (text.to_owned(), false);
    }

    let ellipsis = "...";
    let ellipsis_width = measure_breadcrumb_width(ui, font_id, ellipsis);

    let mut result = String::new();

    for ch in text.chars() {
        let candidate = format!("{result}{ch}");

        if measure_breadcrumb_width(ui, font_id, &candidate) + ellipsis_width > max_width {
            break;
        }

        result.push(ch);
    }

    result.push_str(ellipsis);

    (result, true)
}

fn build_breadcrumbs(path: &Path) -> Vec<Breadcrumb> {
    use std::path::Component;

    if portable::is_portable_path(&path.to_path_buf()) {
        return portable::build_breadcrumb_segments(&path.to_path_buf())
            .unwrap_or_default()
            .into_iter()
            .map(|(label, path)| Breadcrumb { label, path })
            .collect();
    }

    let mut breadcrumbs = Vec::new();
    let mut current = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                current.push(prefix.as_os_str());
                // A bare drive prefix like "D:" (no trailing separator) doesn't
                // mean "the root of D:" to Windows - it means "whatever
                // directory this process last had as its working directory on
                // that drive" (a legacy per-drive-current-directory quirk), so
                // clicking a drive breadcrumb could silently land somewhere
                // else entirely (e.g. wherever the app happened to be
                // launched from). Append the root separator immediately so
                // this breadcrumb's path is unambiguously the drive root.
                current.push(Path::new("\\"));

                breadcrumbs.push(Breadcrumb {
                    label: prefix.as_os_str().to_string_lossy().into_owned(),
                    path: current.clone(),
                });
            }

            Component::RootDir => {
                current.push(Path::new("\\"));
            }

            Component::Normal(name) => {
                current.push(name);

                breadcrumbs.push(Breadcrumb {
                    label: name.to_string_lossy().into_owned(),
                    path: current.clone(),
                });
            }

            _ => {}
        }
    }

    breadcrumbs
}

fn layout_breadcrumbs(
    ui: &egui::Ui,
    breadcrumbs: &[Breadcrumb],
    available_width: f32,
    font_id: &egui::FontId,
) -> Vec<RenderedBreadcrumb> {
    const SEPARATOR_WIDTH: f32 = 18.0;
    const ITEM_PADDING: f32 = 12.0;
    const ICON_WIDTH: f32 = BREADCRUMB_ICON_SIZE + 4.0;

    if breadcrumbs.is_empty() {
        return Vec::new();
    }

    // Measure every breadcrumb once.
    let measured: Vec<RenderedBreadcrumb> = breadcrumbs
        .iter()
        .map(|crumb| RenderedBreadcrumb {
            label: crumb.label.clone(),
            full_label: crumb.label.clone(),
            path: crumb.path.clone(),
            truncated: false,
            is_ellipsis: false,
            width: measure_breadcrumb_width(ui, font_id, &crumb.label) + ITEM_PADDING + ICON_WIDTH,
        })
        .collect();

    // Fast path: everything fits.
    let total_width: f32 = measured.iter().map(|c| c.width).sum::<f32>()
        + SEPARATOR_WIDTH * measured.len().saturating_sub(1) as f32;

    if total_width <= available_width {
        return measured;
    }

    let ellipsis_width = measure_breadcrumb_width(ui, font_id, "...") + ITEM_PADDING;

    let mut result = Vec::new();

    // Always keep the first breadcrumb.
    result.push(measured[0].clone());

    let mut used_width = measured[0].width;

    // Keep as many breadcrumbs from the right as possible.
    let mut right_side = Vec::new();

    for crumb in measured.iter().skip(1).rev() {
        let needed = crumb.width + SEPARATOR_WIDTH;

        // Leave room for:
        //   - separator before "..."
        //   - "..."
        //   - separator after "..."
        let reserved = SEPARATOR_WIDTH + ellipsis_width + SEPARATOR_WIDTH;

        if used_width + reserved + needed <= available_width {
            used_width += needed;
            right_side.push(crumb.clone());
        } else {
            break;
        }
    }

    right_side.reverse();

    let omitted = right_side.len() < measured.len() - 1;

    if omitted {
        result.push(RenderedBreadcrumb {
            label: "...".into(),
            full_label: "...".into(),
            path: measured[0].path.clone(),
            truncated: false,
            is_ellipsis: true,
            width: ellipsis_width,
        });
    }

    result.extend(right_side);

    // Truncate only the final breadcrumb if needed.
    if let Some(last_index) = result.len().checked_sub(1) {
        let consumed: f32 = result[..last_index]
            .iter()
            .map(|c| c.width + SEPARATOR_WIDTH)
            .sum();

        let remaining = (available_width - consumed).max(80.0);

        let full = result[last_index].full_label.clone();

        let (label, truncated) = truncate_breadcrumb_to_width(ui, font_id, &full, remaining);

        result[last_index].label = label;
        result[last_index].truncated = truncated;
    }

    result
}

fn draw_breadcrumb(
    ui: &mut egui::Ui,
    crumb: &RenderedBreadcrumb,
    icon_cache: &IconCache,
    palette: &ThemePalette,
) -> egui::Response {
    let inner = egui::Frame::NONE
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .corner_radius(egui::CornerRadius::same(palette.medium_radius))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;

                if !crumb.is_ellipsis {
                    let icon_size = egui::vec2(BREADCRUMB_ICON_SIZE, BREADCRUMB_ICON_SIZE);
                    if let Some(glyph) = icon_cache.get_custom_folder_icon(&crumb.path, true) {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(glyph)
                                    .size(BREADCRUMB_ICON_SIZE)
                                    .color(palette.icon_colored_hover),
                            )
                            .selectable(false),
                        );
                    } else if let Some(texture) = icon_cache.get(&crumb.path, true) {
                        ui.add(
                            egui::Image::new(&texture)
                                .fit_to_exact_size(icon_size)
                                .tint(palette.icon_colored_hover),
                        );
                    } else {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(regular::FOLDER_SIMPLE)
                                    .size(BREADCRUMB_ICON_SIZE)
                                    .color(palette.icon_colored_hover),
                            )
                            .selectable(false),
                        );
                    }
                }

                let resp = ui.add(
                    egui::Label::new(
                        egui::RichText::new(&crumb.label)
                            .size(palette.text_size + 2.0)
                            .color(palette.text_header_section),
                    )
                    .selectable(false)
                    .sense(egui::Sense::click()),
                );

                if crumb.truncated {
                    resp.on_hover_text(&crumb.full_label)
                } else {
                    resp
                }
            })
            .inner
        });

    inner.response.union(inner.inner)
}

fn handle_breadcrumb_hover(ui: &egui::Ui, resp: &egui::Response) {
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn handle_breadcrumb_click(
    resp: &egui::Response,
    crumb: &RenderedBreadcrumb,
    segments: &[RenderedBreadcrumb],
    drag_active: bool,
    action: &mut ItemViewerNavBarAction,
) {
    if drag_active || !resp.clicked() {
        return;
    }

    if crumb.is_ellipsis {
        if let Some(first) = segments.first() {
            action.nav_to = Some(first.path.clone());
        }
    } else {
        action.nav_to = Some(crumb.path.clone());
    }
}

fn handle_breadcrumb_drag(
    ui: &egui::Ui,
    resp: &egui::Response,
    crumb: &RenderedBreadcrumb,
    segments: &[RenderedBreadcrumb],
    palette: &ThemePalette,
    drag_active: bool,
    hovered_target_ref: Option<&PathBuf>,
    pointer_pos: Option<egui::Pos2>,
    pointer_released: bool,
    breadcrumb_drop_target: &mut Option<PathBuf>,
    action: &mut ItemViewerNavBarAction,
) {
    if !drag_active {
        return;
    }

    let breadcrumb_target_path = if crumb.is_ellipsis {
        segments.first().map(|c| &c.path)
    } else {
        Some(&crumb.path)
    };

    let hovered = breadcrumb_target_path
        .and_then(|target_path| hovered_target_ref.map(|target| target == target_path))
        .unwrap_or_else(|| {
            pointer_pos
                .map(|pointer| resp.rect.contains(pointer))
                .unwrap_or(false)
        });

    if !hovered {
        return;
    }

    let painter = ui
        .ctx()
        .layer_painter(egui::LayerId::new(
            egui::Order::Background,
            ui.id().with("breadcrumb_drop_bg").with(&crumb.path),
        ))
        .with_clip_rect(ui.clip_rect());

    painter.rect_filled(
        resp.rect,
        egui::CornerRadius::same(palette.medium_radius),
        palette.primary_hover,
    );

    if pointer_released {
        let target = if crumb.is_ellipsis {
            segments.first().map(|c| c.path.clone())
        } else {
            Some(crumb.path.clone())
        };

        if let Some(target) = target {
            *breadcrumb_drop_target = Some(target);
            action.move_files_to_breadcrumb_dir_rect = Some(resp.rect);
        }
    }
}

fn merge_toolbar_action(action: &mut ItemViewerNavBarAction, toolbar: ItemViewerNavBarAction) {
    if action.nav.is_none() {
        action.nav = toolbar.nav;
    }

    if action.nav_to.is_none() {
        action.nav_to = toolbar.nav_to;
    }

    action.refresh_current_directory |= toolbar.refresh_current_directory;
    action.create_folder |= toolbar.create_folder;
    action.create_file |= toolbar.create_file;
    action.add_favorite |= toolbar.add_favorite;
    action.remove_favorite |= toolbar.remove_favorite;
    if action.set_display_mode.is_none() {
        action.set_display_mode = toolbar.set_display_mode;
    }
    action.activate_search_box |= toolbar.activate_search_box;
}
