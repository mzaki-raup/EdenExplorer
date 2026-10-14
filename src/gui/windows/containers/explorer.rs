use crate::core::fs::SETTINGS_PATH;
use crate::gui::dragdrop::DropTargets;
use crate::gui::i18n::I18n;
use crate::gui::icons::IconCache;
use crate::gui::theme::ThemePalette;
use crate::gui::windows::containers::enums::ItemViewerAction;
use crate::gui::windows::containers::itemviewer::draw_item_viewer;
use crate::gui::windows::containers::itemviewer_navbar::{
    ADDRESS_BAR_TOTAL_HEIGHT, TOOLBAR_ROW_VERTICAL_PADDING, draw_itemviewer_navigation_bar,
};
use crate::gui::windows::containers::structs::{
    FavoriteItem, ItemViewerDisplayMode, ItemViewerNavBarAction, RenameState, TabView, TagsState,
};
use crate::gui::windows::settings::draw_settings_page;
use crate::gui::windows::structs::{SettingsWindow, ThemeCustomizer};
use crate::gui::utils::format_size;
use eframe::egui;
use egui::{RichText, ScrollArea};
use egui_extras::{Size, StripBuilder};
use egui_phosphor::regular;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use windows::Win32::Foundation::HWND;

const LEFT_MARGIN: f32 = 20.0;
const RIGHT_MARGIN: f32 = 20.0;
const COLUMN_SPACING: f32 = 20.0;
const STATUS_ICON_GAP: f32 = 4.0;
const STATUS_BAR_VPAD: f32 = 6.0;

/// Draws one view's breadcrumb + item viewer + status bar (the content of
/// either half of a split tab, or the whole content area for an unsplit tab).
#[allow(clippy::too_many_arguments)]
pub fn draw_tab_content(
    ui: &mut egui::Ui,
    i18n: &mut I18n,
    icon_cache: &IconCache,
    palette: &ThemePalette,
    hwnd: Option<HWND>,
    view: &mut TabView,
    tab_id: u64,
    is_favorited: bool,
    folder_sizes: &HashMap<
        PathBuf,
        crate::gui::windows::containers::structs::ItemViewerFolderSizeState,
    >,
    clipboard_has_files: bool,
    clipboard_set: &HashSet<PathBuf>,
    clipboard_is_cut: bool,
    show_hidden_files_folders: bool,
    show_item_viewer_icons: bool,
    rename_state: &mut Option<RenameState>,
    file_type_cache: &mut HashMap<String, String>,
    file_size_text_cache: &mut HashMap<PathBuf, (u64, String)>,
    folder_size_text_cache: &mut HashMap<PathBuf, (u64, bool, String)>,
    drive_size_text_cache: &mut HashMap<PathBuf, (u64, u64, String)>,
    external_drag_to_internal_hover: &mut bool,
    drag_active: bool,
    native_drag_active: bool,
    drag_hover_target: Option<PathBuf>,
    tags_state: &mut TagsState,
    theme_customizer: &mut ThemeCustomizer,
    settings_window: &mut SettingsWindow,
    favorites: &mut Vec<FavoriteItem>,
    drop_targets: &mut DropTargets,
    is_focused: bool,
    is_split_pane: bool,
    saved_search_count: usize,
) -> (Option<ItemViewerNavBarAction>, Option<ItemViewerAction>) {
    let viewport_width = ui.available_width();
    let is_drive_view = view.nav.is_root();
    let is_recycle_bin_view = view.nav.is_recycle_bin();
    let is_settings_view = view.nav.current.to_string_lossy() == SETTINGS_PATH;
    let tag_view_group_id = crate::core::fs::parse_tag_view_path(&view.nav.current);
    // A database, or one table/view inside it, is drawn from the file
    // rather than from a directory listing - see `sqlite_ui`.
    let database_view = crate::gui::windows::containers::sqlite_ui::resolve(
        &mut view.sqlite,
        &view.nav.current.clone(),
    );
    if database_view.is_none() {
        // A tab that moved on shouldn't keep a page of rows in memory.
        crate::gui::windows::containers::sqlite_ui::release(&mut view.sqlite);
    }
    let mut hovered_drop_target: Option<PathBuf> = None;
    let mut hovered_drop_target_rect: Option<egui::Rect> = None;
    let mut pending_action: Option<ItemViewerAction> = None;
    let mut tabbar_action = None;

    let font_id = egui::FontId::proportional(palette.text_size);
    let status_height =
        ui.fonts_mut(|f| f.row_height(&font_id)) + 2.0 + 2.0 * STATUS_BAR_VPAD;
    // approximate height of your breadcrumb/tabbar - a split pane stacks the
    // address bar above the toolbar as its own row, so it needs roughly
    // double (each row also reserves `TOOLBAR_ROW_VERTICAL_PADDING` above
    // and below itself).
    //
    // The row containing the address bar must be at least
    // `ADDRESS_BAR_TOTAL_HEIGHT` tall or its own bottom border gets clipped
    // by the file list starting right where this budget says the row
    // should end - previously this was a separately-hardcoded "+8.0" fudge
    // factor that turned out to leave only ~2px of slack once the address
    // bar's real height was accounted for precisely, an amount easily lost
    // to rounding (exactly what happened: the bottom border kept vanishing
    // no matter how the address bar's own code was fixed, because the
    // *budget* around it was still cutting it off by a couple of pixels).
    // Computing this from the address bar's own real constant, with a
    // deliberate `ADDRESS_BAR_SLACK` on top, means the two can't drift out
    // of sync with each other again the way two independent guessed
    // numbers already did once.
    const ADDRESS_BAR_SLACK: f32 = 10.0;
    let address_bar_row_height = ADDRESS_BAR_TOTAL_HEIGHT + ADDRESS_BAR_SLACK;
    let toolbar_row_height = 30.0;
    // The address bar gets its own row above the toolbar in a split pane
    // (half width) and, by default, everywhere else too, so a toolbar with
    // extra buttons (Settings > Toolbar) has the full width.
    let address_bar_own_row =
        is_split_pane || settings_window.current_settings.ui_prefs.address_bar_own_row;
    let tabbar_height = if address_bar_own_row {
        address_bar_row_height + toolbar_row_height + 4.0 * TOOLBAR_ROW_VERTICAL_PADDING
    } else {
        address_bar_row_height.max(toolbar_row_height) + 2.0 * TOOLBAR_ROW_VERTICAL_PADDING
    };

    let old_spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;
    let spring_ctx = ui.ctx().clone();
    let spring_layer = egui::LayerId::new(egui::Order::Foreground, ui.id().with(("spring_bar", tab_id)));

    StripBuilder::new(ui)
        .size(Size::exact(tabbar_height)) // Tabbar
        .size(Size::remainder()) // Item viewer
        .size(Size::exact(status_height)) // Footer
        .vertical(|mut strip| {
            strip.cell(|ui| {
                tabbar_action = Some(draw_itemviewer_navigation_bar(
                    ui,
                    i18n,
                    icon_cache,
                    view,
                    tab_id,
                    palette,
                    is_favorited,
                    drag_active,
                    drag_hover_target.clone(),
                    address_bar_own_row,
                    &tags_state.groups,
                    saved_search_count,
                    settings_window.current_settings.middle_click_opens_new_tab,
                    settings_window.current_settings.tag_icon_style,
                    crate::gui::windows::containers::itemviewer_navbar::BreadcrumbMenus {
                        enabled: settings_window.current_settings.ui_prefs.breadcrumb_dropdowns,
                        show_hidden: settings_window.current_settings.show_hidden_files_folders,
                    },
                    &crate::core::toolbar::drawn_layout(
                        settings_window.current_settings.ui_prefs.toolbar.as_deref(),
                    ),
                ));
            });

            strip.cell(|ui| {
                let content_rect = ui.max_rect();

                if is_settings_view {
                    let (action, tags_item_action) = draw_settings_page(
                        ui,
                        settings_window,
                        i18n,
                        palette,
                        icon_cache,
                        theme_customizer,
                        tags_state,
                        favorites,
                    );
                    if action.is_some() {
                        settings_window.pending_action = action;
                    }
                    if tags_item_action.is_some() {
                        pending_action = tags_item_action;
                    }
                } else if let Some((database, object)) = database_view.clone() {
                    crate::gui::windows::containers::sqlite_ui::draw_sqlite_view(
                        ui,
                        i18n,
                        palette,
                        &mut view.sqlite,
                        &database,
                        object.as_deref(),
                        settings_window.current_settings.double_click_navigates_up,
                        &mut tabbar_action,
                    );
                } else if let Some(group_id) = tag_view_group_id {
                    pending_action = crate::gui::windows::containers::tags::draw_tag_view(
                        ui,
                        i18n,
                        icon_cache,
                        palette,
                        tags_state,
                        group_id,
                        file_type_cache,
                        settings_window.current_settings.date_style,
                        settings_window.current_settings.time_format_24h,
                        &settings_window.current_settings.custom_date_format,
                    );
                } else if view.display_mode == ItemViewerDisplayMode::Gallery {
                    pending_action = draw_item_viewer(
                        ui,
                        i18n,
                        view,
                        folder_sizes,
                        clipboard_has_files,
                        clipboard_set,
                        clipboard_is_cut,
                        is_drive_view,
                        is_recycle_bin_view,
                        show_hidden_files_folders,
                        show_item_viewer_icons,
                        icon_cache,
                        rename_state,
                        palette,
                        file_type_cache,
                        file_size_text_cache,
                        folder_size_text_cache,
                        drive_size_text_cache,
                        external_drag_to_internal_hover,
                        &mut tabbar_action,
                        drag_active,
                        native_drag_active,
                        drag_hover_target.clone(),
                        &mut hovered_drop_target,
                        &mut hovered_drop_target_rect,
                        tags_state,
                        theme_customizer,
                        settings_window,
                        hwnd,
                        is_focused,
                        tab_id,
                        viewport_width,
                    );
                } else {
                    ScrollArea::horizontal()
                        .id_salt(("item_viewer_horizontal_scroll", tab_id))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_width(viewport_width);
                            // The type-to-filter box (drawn by `draw_item_viewer`
                            // via `handle_global_actions`) draws its own border
                            // only a few px inset from this ui's own edge - a
                            // clip rect set exactly to the scroll viewport eats
                            // that border's top/left edge, the same class of bug
                            // already fixed for the navbar's address bar and
                            // search box (see CLAUDE.md). A few px of slack lets
                            // the border render fully without letting real
                            // overflow escape the scroll area visibly.
                            ui.set_clip_rect(ui.clip_rect().expand(3.0));
                            pending_action = draw_item_viewer(
                                ui,
                                i18n,
                                view,
                                folder_sizes,
                                clipboard_has_files,
                                clipboard_set,
                                clipboard_is_cut,
                                is_drive_view,
                                is_recycle_bin_view,
                                show_hidden_files_folders,
                                show_item_viewer_icons,
                                icon_cache,
                                rename_state,
                                palette,
                                file_type_cache,
                                file_size_text_cache,
                                folder_size_text_cache,
                                drive_size_text_cache,
                                external_drag_to_internal_hover,
                                &mut tabbar_action,
                                drag_active,
                                native_drag_active,
                                drag_hover_target.clone(),
                                &mut hovered_drop_target,
                                &mut hovered_drop_target_rect,
                                tags_state,
                                theme_customizer,
                                settings_window,
                                hwnd,
                                is_focused,
                                tab_id,
                                viewport_width,
                            );
                        });
                }

                // A subtle box border around the file/folder view, so a dual-pane
                // split makes it clear at a glance where each pane's view ends.
                // `content_rect`'s top/left/bottom are always internal (the navbar
                // strip above, the sidebar or split divider to the left, the status
                // bar strip below - see the `StripBuilder` this cell comes from),
                // but its *right* edge is this pane's own right bound, which for
                // the last/only pane is also the app's hand-painted outer window
                // border - a real ~3-4px painted band, not a hairline. Drawing
                // flush against it put this stroke's own pixels inside that band,
                // reading as one slightly-thicker line rather than two separate
                // ones. Only the right edge needs pulling in to clear it - the
                // other three sides already had their own margin.
                let mut content_border_rect = content_rect;
                content_border_rect.max.x -= 6.0;
                ui.painter().rect_stroke(
                    content_border_rect,
                    egui::CornerRadius::same(4),
                    egui::Stroke::new(1.5, palette.borders_default),
                    egui::StrokeKind::Inside,
                );
            });

            drop_targets.item_target.target = hovered_drop_target.clone();
            drop_targets.item_target.rect = hovered_drop_target_rect;

            // Spring-loaded folders: resting on a folder (a row or a
            // breadcrumb) while dragging opens it, so the drop can go deeper.
            let spring_ms = settings_window.current_settings.ui_prefs.spring_load_ms;
            if spring_ms > 0 {
                let breadcrumb = tabbar_action.as_ref().and_then(|a| a.drag_hover.clone());
                let hovered = if drag_active {
                    hovered_drop_target
                        .clone()
                        .zip(hovered_drop_target_rect)
                        .or(breadcrumb)
                        .filter(|(path, _)| path != &view.nav.current)
                } else {
                    None
                };
                let step = view.spring.update(
                    hovered.as_ref().map(|(p, _)| p.as_path()),
                    std::time::Instant::now(),
                    std::time::Duration::from_millis(spring_ms as u64),
                );
                if let (Some((_, rect)), Some(remaining)) = (&hovered, step.remaining) {
                    // A thin bar filling up along the folder's bottom edge.
                    let bar = egui::Rect::from_min_size(
                        egui::pos2(rect.left(), rect.bottom() - 2.0),
                        egui::vec2(rect.width() * step.progress, 2.0),
                    );
                    spring_ctx.layer_painter(spring_layer).rect_filled(bar, 1.0, palette.primary);
                    spring_ctx.request_repaint_after(remaining);
                }
                if let Some(open) = step.open {
                    tabbar_action.get_or_insert_with(Default::default).nav_to = Some(open);
                    // The folder opens after this frame; show it right away.
                    spring_ctx.request_repaint();
                }
            }
            drop_targets.breadcrumb_target.target = tabbar_action
                .as_ref()
                .and_then(|a| a.move_files_to_breadcrumb_dir.clone());
            drop_targets.breadcrumb_target.rect = tabbar_action
                .as_ref()
                .and_then(|a| a.move_files_to_breadcrumb_dir_rect);

            strip.cell(|ui| {
                // The strip itself is always drawn - it carries the version
                // (see the far-left block below), which has to stay visible on
                // the drive root ("This PC", the startup view) and Settings too.
                // Only the item/size counters are skipped for those two, since
                // neither has a meaningful count. Every other view (Details,
                // Columns, Gallery, Preview and their variants, Recycle Bin, and
                // tag views) shares this same status bar, including each half of
                // a split pane independently, since `draw_tab_content` is called
                // once per pane.
                {
                    let show_counts = !is_drive_view && !is_settings_view;

                    let text_color = palette.status_bar_text_color;
                    let icon_color = palette.status_bar_icon_color;
                    let font_id = egui::FontId::proportional(palette.text_size);
                    let folder_scanning_enabled =
                        settings_window.current_settings.folder_scanning_enabled;

                    ui.spacing_mut().item_spacing.x = STATUS_ICON_GAP;

                    // An explicit fixed inner margin, rather than relying on
                    // `Align::Center` to split the row's extra height evenly -
                    // the icon glyphs' own line-box metrics carry a lot more
                    // empty space above the visible glyph than below it, so
                    // an equal top/bottom margin still reads as "mostly
                    // touching the bottom" once the glyph's own internal
                    // lopsidedness is added on top. Skewing the margin
                    // (less on top, more on bottom) compensates for that and
                    // was tuned by pixel-sampling a live screenshot.
                    egui::Frame::NONE
                        .fill(palette.status_bar_bg_color)
                        .inner_margin(egui::Margin {
                            left: 0,
                            right: 0,
                            top: (STATUS_BAR_VPAD - 4.0) as i8,
                            bottom: (STATUS_BAR_VPAD + 4.0) as i8,
                        })
                        .show(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if show_counts {
                            let counts = if let Some(group_id) = tag_view_group_id {
                                // The tag view renders its own local row list (see
                                // `tags::draw_tag_view`) rather than populating
                                // `view.files`, so counts have to be computed from the
                                // tag group's own item list instead - `view.files` would
                                // just be stale data left over from whatever real
                                // folder this tab last showed.
                                status_counts_for_tag_group(tags_state, group_id, &view.explorer_state.selected_paths)
                            } else {
                                status_counts_for_view(view, folder_sizes)
                            };
                        ui.add_space(RIGHT_MARGIN);

                        status_counter(
                            ui,
                            regular::FILE,
                            counts.file_count,
                            &font_id,
                            text_color,
                            icon_color,
                            &i18n.tr("status_files_tooltip"),
                        );
                        ui.add_space(COLUMN_SPACING);
                        status_counter(
                            ui,
                            regular::FOLDER_SIMPLE,
                            counts.dir_count,
                            &font_id,
                            text_color,
                            icon_color,
                            &i18n.tr("status_folders_tooltip"),
                        );
                        ui.add_space(COLUMN_SPACING);
                        status_counter(
                            ui,
                            regular::STACK_SIMPLE,
                            counts.dir_count + counts.file_count,
                            &font_id,
                            text_color,
                            icon_color,
                            &i18n.tr("status_total_items_tooltip"),
                        );
                        ui.add_space(COLUMN_SPACING);
                        status_size(
                            ui,
                            regular::HARD_DRIVE,
                            counts.total_size,
                            &font_id,
                            text_color,
                            icon_color,
                            &i18n.tr(if folder_scanning_enabled {
                                "status_total_size_tooltip"
                            } else {
                                "status_total_size_tooltip_files_only"
                            }),
                        );

                        // Free space on the drive holding this folder (not
                        // for virtual views like the Recycle Bin or tags).
                        if tag_view_group_id.is_none()
                            && let Some((total, free)) = cached_drive_space(&view.nav.current)
                        {
                            ui.add_space(COLUMN_SPACING);
                            let tooltip = format!(
                                "{} ({} {})",
                                i18n.tr("status_free_space_tooltip"),
                                format_size(total),
                                i18n.tr("status_total"),
                            );
                            ui.label(
                                RichText::new(format!("{} {}", format_size(free), i18n.tr("status_free")))
                                    .font(font_id.clone())
                                    .color(text_color),
                            )
                            .on_hover_text(&tooltip);
                            ui.label(RichText::new(regular::DATABASE).font(font_id.clone()).color(icon_color))
                                .on_hover_text(&tooltip);
                        }

                        if counts.selected_count > 0 {
                            ui.add_space(COLUMN_SPACING);
                            let selected_label = if counts.selected_count == 1 {
                                i18n.tr("item_capital")
                            } else {
                                i18n.tr("items_capital")
                            };
                            let selected_text = format!(
                                "{} {selected_label} {} \u{2022} {}",
                                counts.selected_count,
                                i18n.tr("selected"),
                                format_size(counts.selected_size),
                            );
                            ui.label(RichText::new(selected_text).font(font_id.clone()).color(text_color));
                        }

                        // Active type-to-filter: what's being matched, how
                        // many items it leaves, and a button to clear it.
                        let filter = &view.item_viewer_filter_state;
                        if tag_view_group_id.is_none() && filter.narrows() {
                            ui.add_space(COLUMN_SPACING);
                            let shown = counts.dir_count + counts.file_count;
                            let chip = egui::Frame::NONE
                                .fill(palette.primary.gamma_multiply(0.18))
                                .stroke(egui::Stroke::new(1.0, palette.primary.gamma_multiply(0.6)))
                                .corner_radius(egui::CornerRadius::same(palette.small_radius))
                                .inner_margin(egui::Margin::symmetric(6, 1))
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    let clear = ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new(regular::X).font(font_id.clone()).color(text_color),
                                            )
                                            .frame(false),
                                        )
                                        .on_hover_text(i18n.tr("status_clear_filter"))
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .clicked();
                                    ui.label(
                                        RichText::new(format!(
                                            "{}{} \u{2022} {} {} {}",
                                            i18n.tr("status_filter"),
                                            // A kind chip alone has no text to quote.
                                            if filter.query.is_empty() {
                                                String::new()
                                            } else {
                                                format!(" \"{}\"", filter.query)
                                            },
                                            shown,
                                            i18n.tr("status_of"),
                                            view.files.len(),
                                        ))
                                        .font(font_id.clone())
                                        .color(text_color),
                                    );
                                    ui.label(RichText::new(regular::FUNNEL).font(font_id.clone()).color(palette.primary));
                                    clear
                                });
                            if chip.inner {
                                view.item_viewer_filter_state = Default::default();
                            }
                        }

                        // Flat view: how many files it lists, and a button
                        // to go back to the normal view.
                        if view.is_flat() {
                            ui.add_space(COLUMN_SPACING);
                            let truncated =
                                view.flat_truncated.load(std::sync::atomic::Ordering::Relaxed);
                            let chip = egui::Frame::NONE
                                .fill(palette.primary.gamma_multiply(0.18))
                                .stroke(egui::Stroke::new(1.0, palette.primary.gamma_multiply(0.6)))
                                .corner_radius(egui::CornerRadius::same(palette.small_radius))
                                .inner_margin(egui::Margin::symmetric(6, 1))
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    let close = ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new(regular::X).font(font_id.clone()).color(text_color),
                                            )
                                            .frame(false),
                                        )
                                        .on_hover_text(i18n.tr("status_flat_view_off"))
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .clicked();
                                    let mut text = format!(
                                        "{} \u{2022} {} {}",
                                        i18n.tr("status_flat_view"),
                                        view.files.len(),
                                        i18n.tr("status_flat_view_files"),
                                    );
                                    if truncated {
                                        text.push_str(&format!(" ({})", i18n.tr("status_flat_view_truncated")));
                                    }
                                    ui.label(RichText::new(text).font(font_id.clone()).color(text_color));
                                    ui.label(RichText::new(regular::STACK).font(font_id.clone()).color(palette.primary));
                                    close
                                });
                            if chip.inner {
                                tabbar_action.get_or_insert_with(Default::default).toolbar_command =
                                    Some(crate::core::toolbar::ToolbarItem::FlatView);
                            }
                        }

                        // Git: the branch, and how many files have changes.
                        if let Some(git) = view.git.as_deref() {
                            ui.add_space(COLUMN_SPACING);
                            let mut tooltip = format!("{}\n{}", i18n.tr("status_git_repo"), git.workdir.display());
                            if let Some(error) = &git.error {
                                tooltip.push_str(&format!("\n{error}"));
                            }
                            if git.changed > 0 {
                                ui.label(
                                    RichText::new(format!("{} {}", git.changed, i18n.tr("status_git_changes")))
                                        .font(font_id.clone())
                                        .color(crate::gui::windows::containers::itemviewer_helper::git_state_color(
                                            crate::core::git::GitState::Modified,
                                            ui.visuals().dark_mode,
                                        )),
                                )
                                .on_hover_text(&tooltip);
                                ui.label(RichText::new("\u{2022}").font(font_id.clone()).color(text_color));
                            }
                            ui.label(RichText::new(&git.branch).font(font_id.clone()).color(text_color))
                                .on_hover_text(&tooltip);
                            ui.label(RichText::new(regular::GIT_BRANCH).font(font_id.clone()).color(icon_color))
                                .on_hover_text(&tooltip);
                        }
                        }

                        // Far left, so it never collides with the right-aligned
                        // counters above however many of them a view shows. The
                        // remaining space after a right_to_left pass is the left
                        // end of the row, so a left_to_right layout over it
                        // starts flush against the row's own left edge.
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add_space(LEFT_MARGIN);
                            let version = format!("v{}", env!("CARGO_PKG_VERSION"));
                            let tooltip =
                                format!("{}: {}", i18n.tr("about_current_version"), version);
                            ui.label(RichText::new(regular::INFO).font(font_id.clone()).color(icon_color))
                                .on_hover_text(&tooltip);
                            ui.label(RichText::new(version).font(font_id.clone()).color(text_color))
                                .on_hover_text(&tooltip);

                            // An available update, published here once per frame
                            // by `update_ui::poll_update_events` - a memory flag
                            // rather than another `draw_tab_content` parameter,
                            // since this is several call sites deep.
                            let newer = ui.ctx().memory(|mem| {
                                mem.data
                                    .get_temp::<Option<String>>(egui::Id::new(
                                        crate::gui::windows::update_ui::UPDATE_BADGE_MEMORY_ID,
                                    ))
                                    .flatten()
                            });
                            if let Some(newer) = newer {
                                ui.add_space(8.0);
                                let label =
                                    format!("{} v{newer}", i18n.tr("update_status_available"));
                                let chip = egui::Frame::NONE
                                    .fill(palette.primary.gamma_multiply(0.18))
                                    .stroke(egui::Stroke::new(
                                        1.0,
                                        palette.primary.gamma_multiply(0.6),
                                    ))
                                    .corner_radius(egui::CornerRadius::same(palette.small_radius))
                                    .inner_margin(egui::Margin::symmetric(6, 1))
                                    .show(ui, |ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        ui.label(
                                            RichText::new(regular::ARROW_CIRCLE_UP)
                                                .font(font_id.clone())
                                                .color(palette.primary),
                                        );
                                        // A bare label inside a clickable row
                                        // steals that row's own clicks unless it
                                        // opts out - see CLAUDE.md.
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&label)
                                                    .font(font_id.clone())
                                                    .color(text_color),
                                            )
                                            .selectable(false),
                                        );
                                    });
                                if ui
                                    .interact(
                                        chip.response.rect,
                                        ui.id().with("update_badge"),
                                        egui::Sense::click(),
                                    )
                                    .on_hover_text(i18n.tr("update_status_available_hint"))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    ui.ctx().memory_mut(|mem| {
                                        mem.data.insert_temp(
                                            egui::Id::new(
                                                crate::gui::windows::update_ui::UPDATE_CLICKED_MEMORY_ID,
                                            ),
                                            true,
                                        );
                                    });
                                }
                            }
                        });
                    });
                        });
                }
            });
        });

    ui.spacing_mut().item_spacing.y = old_spacing;

    (tabbar_action, pending_action)
}

/// Free/total space of the drive holding `path`, looked up at most every
/// 10 seconds per drive (the status bar is drawn every frame).
fn cached_drive_space(path: &std::path::Path) -> Option<(u64, u64)> {
    use std::cell::RefCell;
    use std::time::{Duration, Instant};
    thread_local! {
        static CACHE: RefCell<HashMap<PathBuf, (Instant, Option<(u64, u64)>)>> =
            RefCell::new(HashMap::new());
    }
    if !path.is_absolute() {
        return None;
    }
    let root = path.ancestors().last()?.to_path_buf();
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((at, space)) = cache.get(&root)
            && at.elapsed() < Duration::from_secs(10)
        {
            return *space;
        }
        let space = crate::core::fs::get_drive_space(&path.to_path_buf());
        cache.insert(root, (Instant::now(), space));
        space
    })
}

/// The status bar's four right-aligned counters plus the selection summary,
/// gathered into one place so both the normal-directory and tag-view sources
/// (see `status_counts_for_view`/`status_counts_for_tag_group`) can feed the
/// same rendering code.
#[derive(Clone, Copy)]
struct StatusCounts {
    dir_count: usize,
    file_count: usize,
    /// Sum of every visible file's size, plus every visible folder's size
    /// where a folder-size scan has already produced one (see
    /// `folder_sizes` in `ItemViewerFolderSizeState`) - folders with no scan
    /// result yet (scanning disabled, or still in progress) simply don't
    /// contribute, which is why the size tooltip differs when folder-size
    /// scanning is off.
    total_size: u64,
    selected_count: usize,
    selected_size: u64,
}

fn status_counts_for_view(
    view: &TabView,
    folder_sizes: &HashMap<PathBuf, crate::gui::windows::containers::structs::ItemViewerFolderSizeState>,
) -> StatusCounts {
    let mut dir_count = 0usize;
    let mut file_count = 0usize;
    let mut total_size = 0u64;

    let item_size = |file: &crate::core::fs::FileItem| -> u64 {
        if file.is_dir {
            folder_sizes.get(&file.path).map(|s| s.bytes).unwrap_or(0)
        } else {
            file.file_size.unwrap_or(0)
        }
    };

    // This runs every frame, for every pane - so once per frame per open
    // pane, which a tab can now have three of. The selected size used to be
    // a second pass over *all* of `view.files` (not just the visible ones,
    // and a search or flat view can hold tens of thousands); it is folded
    // into the pass below instead, costing one hash lookup per visible row.
    // Nothing is lost by only looking at visible rows: `draw_item_viewer`
    // drops the selection whenever a filter change would hide any of it, so
    // a selected item is always one of these.
    let selected_paths = &view.explorer_state.selected_paths;
    let selected_count = selected_paths.len();
    let mut selected_size = 0u64;

    for &idx in &view.item_viewer_filter_state.cached_indices {
        let file = &view.files[idx];
        if file.is_dir {
            dir_count += 1;
        } else {
            file_count += 1;
        }
        let size = item_size(file);
        total_size += size;
        if selected_count > 0 && selected_paths.contains(&file.path) {
            selected_size += size;
        }
    }

    StatusCounts {
        dir_count,
        file_count,
        total_size,
        selected_count,
        selected_size,
    }
}

/// The last counts worked out for a tag group, and what they were worked
/// out from.
///
/// Counting a tag group needs a `metadata` call per item - unlike an ordinary
/// folder, whose counts come from `view.files`, already in memory - and the
/// status bar is drawn **every frame**. A group of a hundred items was
/// therefore making six thousand filesystem calls a second just to keep a
/// line of text up to date, for a number that only changes when the group or
/// the selection does.
///
/// Hashing the inputs each frame is `O(bytes)` where the old behaviour was
/// `O(items)` *syscalls*, so even for a large group this is far cheaper -
/// and it stays correct when a group is edited, unlike a timer.
static TAG_COUNTS_CACHE: std::sync::Mutex<Option<(u64, StatusCounts)>> =
    std::sync::Mutex::new(None);

/// What the counts depend on: which group, its items, and what is selected.
fn tag_counts_signature(
    tags_state: &TagsState,
    group_id: u64,
    selected_paths: &HashSet<PathBuf>,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    group_id.hash(&mut hasher);
    if let Some(group) = tags_state.groups.iter().find(|g| g.id == group_id) {
        for path in &group.items {
            path.hash(&mut hasher);
        }
    }
    // A set has no order, so combine each member commutatively rather than
    // hashing the iteration order - which can differ between frames for the
    // same selection.
    let mut selection = 0u64;
    for path in selected_paths {
        let mut each = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut each);
        selection ^= each.finish();
    }
    selection.hash(&mut hasher);
    hasher.finish()
}

fn status_counts_for_tag_group(
    tags_state: &TagsState,
    group_id: u64,
    selected_paths: &HashSet<PathBuf>,
) -> StatusCounts {
    let signature = tag_counts_signature(tags_state, group_id, selected_paths);
    if let Ok(cache) = TAG_COUNTS_CACHE.lock()
        && let Some((cached_signature, counts)) = cache.as_ref()
        && *cached_signature == signature
    {
        return *counts;
    }

    let counts = count_tag_group(tags_state, group_id, selected_paths);
    if let Ok(mut cache) = TAG_COUNTS_CACHE.lock() {
        *cache = Some((signature, counts));
    }
    counts
}

/// The actual walk, run only when `status_counts_for_tag_group`'s inputs
/// have changed.
fn count_tag_group(
    tags_state: &TagsState,
    group_id: u64,
    selected_paths: &HashSet<PathBuf>,
) -> StatusCounts {
    let mut counts = StatusCounts {
        dir_count: 0,
        file_count: 0,
        total_size: 0,
        selected_count: 0,
        selected_size: 0,
    };

    let Some(group) = tags_state.groups.iter().find(|g| g.id == group_id) else {
        return counts;
    };

    for path in &group.items {
        let Ok(metadata) = std::fs::metadata(path) else {
            continue;
        };
        let is_selected = selected_paths.contains(path);
        if metadata.is_dir() {
            counts.dir_count += 1;
        } else {
            counts.file_count += 1;
            counts.total_size += metadata.len();
            if is_selected {
                counts.selected_size += metadata.len();
            }
        }
        if is_selected {
            counts.selected_count += 1;
        }
    }

    counts
}

fn status_counter(
    ui: &mut egui::Ui,
    icon: &str,
    count: usize,
    font_id: &egui::FontId,
    text_color: egui::Color32,
    icon_color: egui::Color32,
    tooltip: &str,
) {
    // Added in right-to-left order (icon first) so the icon ends up on the
    // right and the number to its left, matching how this status bar has
    // always read: "12 🗎" rather than "🗎 12".
    ui.label(RichText::new(icon).font(font_id.clone()).color(icon_color))
        .on_hover_text(tooltip);
    ui.label(RichText::new(count.to_string()).font(font_id.clone()).color(text_color))
        .on_hover_text(tooltip);
}

fn status_size(
    ui: &mut egui::Ui,
    icon: &str,
    total_size: u64,
    font_id: &egui::FontId,
    text_color: egui::Color32,
    icon_color: egui::Color32,
    tooltip: &str,
) {
    // Reverse order from `status_counter`: a byte-size reads more naturally
    // as "💾 1.2 GB" (icon first) than trailing the icon after the number.
    ui.label(RichText::new(format_size(total_size)).font(font_id.clone()).color(text_color))
        .on_hover_text(tooltip);
    ui.label(RichText::new(icon).font(font_id.clone()).color(icon_color))
        .on_hover_text(tooltip);
}

#[cfg(test)]
mod tag_counts_tests {
    use super::*;
    use crate::gui::windows::containers::structs::TagGroup;

    fn group(id: u64, items: &[&str]) -> TagGroup {
        TagGroup {
            id,
            name: "g".into(),
            color: egui::Color32::WHITE,
            items: items.iter().map(PathBuf::from).collect(),
        }
    }

    fn state(groups: Vec<TagGroup>) -> TagsState {
        TagsState {
            groups,
            ..Default::default()
        }
    }

    fn selection(paths: &[&str]) -> HashSet<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    /// The cache is only sound if the signature moves whenever the counts
    /// would. These pin that, because the alternative - a stale status bar
    /// showing the wrong number of items - is a worse bug than the
    /// per-frame `metadata` calls the cache exists to avoid.
    #[test]
    fn the_same_inputs_give_the_same_signature() {
        let tags = state(vec![group(1, &["a", "b"])]);
        let selected = selection(&["a"]);
        assert_eq!(
            tag_counts_signature(&tags, 1, &selected),
            tag_counts_signature(&tags, 1, &selected)
        );
    }

    #[test]
    fn changing_the_selection_changes_the_signature() {
        let tags = state(vec![group(1, &["a", "b"])]);
        let before = tag_counts_signature(&tags, 1, &selection(&["a"]));
        // Same number of items selected, a different one - the case a
        // length-based cache key would miss.
        let after = tag_counts_signature(&tags, 1, &selection(&["b"]));
        assert_ne!(before, after);
        assert_ne!(before, tag_counts_signature(&tags, 1, &selection(&[])));
    }

    #[test]
    fn changing_the_group_contents_changes_the_signature() {
        let selected = selection(&["a"]);
        let before = tag_counts_signature(&state(vec![group(1, &["a", "b"])]), 1, &selected);
        // Same count, different member.
        let renamed = tag_counts_signature(&state(vec![group(1, &["a", "c"])]), 1, &selected);
        let shorter = tag_counts_signature(&state(vec![group(1, &["a"])]), 1, &selected);
        assert_ne!(before, renamed);
        assert_ne!(before, shorter);
    }

    #[test]
    fn two_groups_do_not_share_a_signature() {
        let tags = state(vec![group(1, &["a"]), group(2, &["a"])]);
        let selected = selection(&["a"]);
        assert_ne!(
            tag_counts_signature(&tags, 1, &selected),
            tag_counts_signature(&tags, 2, &selected)
        );
    }

    #[test]
    fn the_selection_is_combined_without_depending_on_iteration_order() {
        // `HashSet` has no order, and it can differ between frames for the
        // same selection - so the signature must not depend on it, or the
        // counts would be recomputed at random (or worse, flicker).
        let tags = state(vec![group(1, &["a", "b", "c"])]);
        let one = tag_counts_signature(&tags, 1, &selection(&["a", "b", "c"]));
        let two = tag_counts_signature(&tags, 1, &selection(&["c", "a", "b"]));
        assert_eq!(one, two);
    }
}
