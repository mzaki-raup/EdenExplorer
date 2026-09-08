use crate::core::fs::FileItem;
use crate::core::preview::{PreviewPayload, PreviewService};
use crate::core::video::VideoPreviewService;
use crate::gui::i18n::I18n;
use crate::gui::icons::IconCache;
use crate::gui::theme::ThemePalette;
use crate::gui::utils::truncate_item_text;
use crate::gui::windows::containers::enums::ItemViewerAction;
use crate::gui::windows::containers::structs::ExplorerState;
use chrono::{DateTime, Local};
use eframe::egui;
use egui_extras::{Size, StripBuilder};
use egui_phosphor::regular;
use std::path::{Path, PathBuf};

pub const PREVIEW_PANE_WIDTH: f32 = 560.0;
const ROW_HEIGHT: f32 = 24.0;

/// A layout with a simple file list on the left and a preview pane on the right
/// (showing the content of the selected file - text, image, PDF, or Word document).
#[allow(clippy::too_many_arguments)]
pub fn draw_preview_view(
    ui: &mut egui::Ui,
    i18n: &I18n,
    files: &[FileItem],
    filtered_indices: &[usize],
    explorer_state: &mut ExplorerState,
    preview_service: &mut PreviewService,
    video_service: &mut VideoPreviewService,
    preview_selection: &mut Option<PathBuf>,
    icon_cache: &IconCache,
    palette: &ThemePalette,
) -> Option<ItemViewerAction> {
    preview_service.pump(ui.ctx());

    let mut action = None;

    StripBuilder::new(ui)
        .size(Size::remainder())
        .size(Size::exact(PREVIEW_PANE_WIDTH))
        .horizontal(|mut strip| {
            strip.cell(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt("preview_view_list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Left padding to match every other layout's file/folder
                        // list, which otherwise sits flush against the pane's edge.
                        ui.horizontal_top(|ui| {
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                for &idx in filtered_indices {
                                    let file = &files[idx];
                                    let is_selected =
                                        explorer_state.selected_paths.contains(&file.path);

                                    let (rect, resp) = ui.allocate_exact_size(
                                        egui::vec2(ui.available_width(), ROW_HEIGHT),
                                        egui::Sense::click(),
                                    );

                                    if is_selected {
                                        ui.painter().rect_filled(
                                            rect,
                                            egui::CornerRadius::same(palette.small_radius),
                                            palette.primary_active,
                                        );
                                    } else if resp.hovered() {
                                        ui.painter().rect_filled(
                                            rect,
                                            egui::CornerRadius::same(palette.small_radius),
                                            palette.primary_hover,
                                        );
                                    }

                                    const ICON_SIZE: f32 = 16.0;
                                    let icon_area = egui::Rect::from_min_size(
                                        rect.left_center() + egui::vec2(4.0, -ICON_SIZE * 0.5),
                                        egui::vec2(ICON_SIZE, ICON_SIZE),
                                    );
                                    let text_x = if let Some(glyph) =
                                        icon_cache.get_custom_folder_icon(&file.path, file.is_dir)
                                    {
                                        ui.painter().text(
                                            icon_area.center(),
                                            egui::Align2::CENTER_CENTER,
                                            glyph,
                                            egui::FontId::proportional(ICON_SIZE),
                                            palette.icon_colored_hover,
                                        );
                                        icon_area.right() + 6.0
                                    } else if let Some(texture) =
                                        icon_cache.get(&file.path, file.is_dir)
                                    {
                                        ui.painter().image(
                                            (&texture).into(),
                                            icon_area,
                                            egui::Rect::from_min_size(
                                                egui::pos2(0.0, 0.0),
                                                egui::vec2(1.0, 1.0),
                                            ),
                                            palette.icon_colored_hover,
                                        );
                                        icon_area.right() + 6.0
                                    } else {
                                        rect.left() + 8.0
                                    };
                                    let max_text_width = (rect.right() - text_x - 4.0).max(0.0);
                                    let name_font_id =
                                        egui::FontId::proportional(palette.text_size);
                                    let text_color = ui.visuals().text_color();
                                    let (display_name, _) = truncate_item_text(
                                        ui,
                                        &file.name,
                                        max_text_width,
                                        &name_font_id,
                                        text_color,
                                    );

                                    ui.painter().text(
                                        egui::pos2(text_x, rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        display_name,
                                        name_font_id,
                                        text_color,
                                    );

                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }

                                    if resp.clicked() {
                                        explorer_state.selected_paths.clear();
                                        explorer_state.selected_paths.insert(file.path.clone());
                                        explorer_state.selection_focus = Some(idx);
                                        explorer_state.selection_anchor = Some(idx);
                                        *preview_selection = Some(file.path.clone());
                                        if !file.is_dir {
                                            preview_service.request(&file.path);
                                        }
                                    }

                                    if resp.double_clicked() {
                                        action = Some(if file.is_dir {
                                            ItemViewerAction::Open(file.path.clone())
                                        } else {
                                            ItemViewerAction::OpenWithDefault(vec![
                                                file.path.clone(),
                                            ])
                                        });
                                    }
                                }

                                if filtered_indices.is_empty() {
                                    ui.add_space(8.0);
                                    ui.weak(i18n.tr("folder_is_empty"));
                                }
                            });
                        });
                    });
            });

            strip.cell(|ui| {
                let divider_rect = ui.max_rect();
                ui.painter().vline(
                    divider_rect.left(),
                    divider_rect.y_range(),
                    egui::Stroke::new(1.0, palette.borders_default),
                );
                draw_preview_pane(
                    ui,
                    i18n,
                    preview_service,
                    video_service,
                    preview_selection.as_deref(),
                    palette,
                );
            });
        });

    action
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PreviewPaneTab {
    Preview,
    Details,
}

/// Renders `selection` (if any) inside a small "Preview" / "Details" tab strip:
/// the Preview tab shows the file's content (text, image, or the extracted text
/// of a PDF/Word document); the Details tab shows file metadata (size,
/// dimensions for images, created/modified dates).
#[allow(clippy::too_many_arguments)]
pub fn draw_preview_pane(
    ui: &mut egui::Ui,
    i18n: &I18n,
    preview_service: &mut PreviewService,
    video_service: &mut VideoPreviewService,
    selection: Option<&Path>,
    palette: &ThemePalette,
) {
    let Some(path) = selection else {
        egui::Frame::NONE
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.weak(i18n.tr("preview_select_a_file"));
                });
            });
        return;
    };

    let tab_id = egui::Id::new("preview_pane_active_tab");
    let mut active_tab = ui
        .ctx()
        .data_mut(|d| *d.get_temp_mut_or(tab_id, PreviewPaneTab::Preview));

    egui::Frame::NONE
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            ui.label(egui::RichText::new(file_name).strong());
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                draw_preview_pane_tab_button(
                    ui,
                    &mut active_tab,
                    PreviewPaneTab::Preview,
                    &i18n.tr("preview_tab_preview"),
                    palette,
                );
                ui.add_space(12.0);
                draw_preview_pane_tab_button(
                    ui,
                    &mut active_tab,
                    PreviewPaneTab::Details,
                    &i18n.tr("preview_tab_details"),
                    palette,
                );
            });
            ui.separator();
            ui.add_space(8.0);

            // The selection highlight otherwise reuses the same subtle hover
            // tint as list rows/buttons - too low-contrast to notice, and
            // (since egui recolors selected glyphs to `selection.stroke.color`)
            // the wrong text color too. Use a solid, saturated fill with an
            // explicitly light/dark text color picked for contrast against it,
            // instead of another theme-derived color that might be close in
            // tone to the fill.
            ui.visuals_mut().selection.bg_fill = egui::Color32::from_rgb(
                palette.primary.r(),
                palette.primary.g(),
                palette.primary.b(),
            );
            let fill_luminance = 0.299 * palette.primary.r() as f32
                + 0.587 * palette.primary.g() as f32
                + 0.114 * palette.primary.b() as f32;
            ui.visuals_mut().selection.stroke.color = if fill_luminance > 140.0 {
                egui::Color32::BLACK
            } else {
                egui::Color32::WHITE
            };

            match active_tab {
                PreviewPaneTab::Preview => {
                    draw_preview_content(ui, i18n, preview_service, video_service, path, palette)
                }
                PreviewPaneTab::Details => draw_preview_details(ui, i18n, preview_service, path),
            }
        });

    ui.ctx().data_mut(|d| d.insert_temp(tab_id, active_tab));
}

/// Draws a single "Preview"/"Details" tab label with an explicit, always-legible
/// text color and an underline when active - avoids relying on egui's default
/// selectable-label selection colors, which can end up low-contrast against
/// this app's custom theme.
fn draw_preview_pane_tab_button(
    ui: &mut egui::Ui,
    active_tab: &mut PreviewPaneTab,
    this_tab: PreviewPaneTab,
    label: &str,
    palette: &ThemePalette,
) {
    let is_active = *active_tab == this_tab;
    let color = if is_active {
        palette.primary
    } else {
        ui.visuals().text_color()
    };

    let resp = ui.add(
        egui::Label::new(
            egui::RichText::new(label)
                .color(color)
                .size(palette.text_size)
                .strong(),
        )
        .selectable(false)
        .sense(egui::Sense::click()),
    );

    if is_active {
        let underline_y = resp.rect.bottom() + 2.0;
        ui.painter().line_segment(
            [
                egui::pos2(resp.rect.left(), underline_y),
                egui::pos2(resp.rect.right(), underline_y),
            ],
            egui::Stroke::new(2.0, palette.primary),
        );
    }

    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    if resp.clicked() {
        *active_tab = this_tab;
    }
}

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "avi", "wmv", "mkv", "webm", "flv", "mpg", "mpeg", "3gp", "3g2", "ts",
    "m2ts",
];

fn is_video_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| VIDEO_EXTENSIONS.iter().any(|v| v.eq_ignore_ascii_case(ext)))
}

fn draw_preview_content(
    ui: &mut egui::Ui,
    i18n: &I18n,
    preview_service: &mut PreviewService,
    video_service: &mut VideoPreviewService,
    path: &Path,
    palette: &ThemePalette,
) {
    if is_video_extension(path) {
        video_service.set_current(path);

        if let Some(error) = video_service.error() {
            ui.add_space(8.0);
            ui.colored_label(egui::Color32::from_rgb(220, 90, 90), error.to_string());
            return;
        }

        if let Some(texture) = video_service.texture_for(ui.ctx()) {
            const CONTROLS_HEIGHT: f32 = 28.0;
            let avail = ui.available_size();
            let video_area_height = (avail.y - CONTROLS_HEIGHT - 6.0).max(0.0);

            ui.allocate_ui(egui::vec2(avail.x, video_area_height), |ui| {
                let resp = ui.add(
                    egui::Image::new(&texture)
                        .max_size(ui.available_size())
                        .shrink_to_fit()
                        .sense(egui::Sense::click()),
                );
                let clicked = resp.clicked();
                resp.on_hover_text(i18n.tr("preview_video_toggle_play"))
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if clicked {
                    video_service.toggle_play_pause();
                }
            });

            ui.add_space(6.0);

            ui.horizontal(|ui| {
                let is_paused = video_service.is_paused();
                let icon = if is_paused {
                    regular::PLAY
                } else {
                    regular::PAUSE
                };
                if ui
                    .add(egui::Button::new(egui::RichText::new(icon).size(16.0)))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    video_service.toggle_play_pause();
                }

                if let Some((current, duration)) = video_service.progress() {
                    ui.label(
                        egui::RichText::new(format_video_time(current))
                            .monospace()
                            .size(palette.tooltip_text_size),
                    );

                    let mut fraction = (current / duration).clamp(0.0, 1.0) as f32;
                    let slider_width = (ui.available_width() - 60.0).max(20.0);
                    let slider_resp = ui.add_sized(
                        egui::vec2(slider_width, 18.0),
                        egui::Slider::new(&mut fraction, 0.0..=1.0).show_value(false),
                    );
                    if slider_resp.changed() {
                        video_service.seek(fraction as f64 * duration);
                    }

                    ui.label(
                        egui::RichText::new(format_video_time(duration))
                            .monospace()
                            .size(palette.tooltip_text_size),
                    );
                }
            });
        } else {
            ui.weak(i18n.tr("preview_loading"));
        }
        return;
    }

    // Image/Animated payloads can be large (an animated GIF's decoded frames
    // especially), so those are handled without cloning the payload itself -
    // `texture_for`/`animated_texture_for` read straight from the cache.
    if matches!(
        preview_service.get(path),
        Some(PreviewPayload::Image { .. })
    ) {
        if let Some(texture) = preview_service.texture_for(ui.ctx(), path) {
            let avail = ui.available_size();
            egui::ScrollArea::both()
                .id_salt("preview_pane_image")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(egui::Image::new(&texture).max_size(avail).shrink_to_fit());
                });
        } else {
            ui.weak(i18n.tr("preview_loading"));
        }
        return;
    }

    if matches!(
        preview_service.get(path),
        Some(PreviewPayload::Animated { .. })
    ) {
        if let Some(texture) = preview_service.animated_texture_for(ui.ctx(), path) {
            let avail = ui.available_size();
            egui::ScrollArea::both()
                .id_salt("preview_pane_animated")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(egui::Image::new(&texture).max_size(avail).shrink_to_fit());
                });
        } else {
            ui.weak(i18n.tr("preview_loading"));
        }
        return;
    }

    let payload = preview_service.get(path).cloned();

    match payload {
        Some(PreviewPayload::Text(text)) => {
            egui::ScrollArea::both()
                .id_salt("preview_pane_text")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(text)
                                .monospace()
                                .size(palette.text_size),
                        )
                        .wrap(),
                    );
                });
        }
        Some(PreviewPayload::Markdown(markdown)) => {
            egui::ScrollArea::both()
                .id_salt("preview_pane_markdown")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    draw_markdown(ui, palette, &markdown);
                });
        }
        Some(PreviewPayload::Image { .. }) | Some(PreviewPayload::Animated { .. }) => {
            // Handled above before this clone; unreachable in practice.
        }
        Some(PreviewPayload::Archive { entries, truncated }) => {
            egui::ScrollArea::both()
                .id_salt("preview_pane_archive")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for entry in &entries {
                        ui.horizontal(|ui| {
                            ui.add_space(entry.depth as f32 * 16.0);
                            let icon = if entry.is_dir {
                                regular::FOLDER_SIMPLE
                            } else {
                                regular::FILE
                            };
                            ui.colored_label(palette.icon_colored_hover, icon);
                            ui.label(egui::RichText::new(&entry.name).size(palette.text_size));
                            if !entry.is_dir {
                                ui.weak(
                                    egui::RichText::new(format_file_size(entry.size))
                                        .size(palette.tooltip_text_size),
                                );
                            }
                        });
                    }

                    if truncated {
                        ui.add_space(8.0);
                        ui.weak(i18n.tr("preview_archive_truncated"));
                    }
                });
        }
        Some(PreviewPayload::Unsupported(message)) => {
            ui.add_space(8.0);
            ui.weak(message);
        }
        Some(PreviewPayload::Error(message)) => {
            ui.add_space(8.0);
            ui.colored_label(egui::Color32::from_rgb(220, 90, 90), message);
        }
        None => {
            preview_service.request(path);
            ui.add_space(8.0);
            ui.weak(i18n.tr("preview_loading"));
        }
    }
}

/// Renders Markdown source (headings, bold/italic/strikethrough, inline and
/// fenced code, lists, block quotes, rules, task list checkboxes, and simple
/// tables) instead of showing it as raw text. Hand-rolled on top of
/// `pulldown-cmark` (a pure parser with no UI dependency of its own) rather
/// than a ready-made egui markdown widget, since none of those are published
/// against this app's egui version. HTML blocks, footnotes, and definition
/// lists are silently skipped - rare in the kind of README/notes-style
/// Markdown this is meant for.
fn draw_markdown(ui: &mut egui::Ui, palette: &ThemePalette, markdown: &str) {
    use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

    let base_size = palette.text_size;
    let mono_font = egui::FontId::monospace(base_size);
    let text_color = ui.visuals().text_color();
    let strong_color = ui.visuals().strong_text_color();
    let weak_color = ui.visuals().weak_text_color();
    let heading_color = palette.text_header_section;
    let code_bg = ui.visuals().extreme_bg_color;

    let mut bold_depth = 0u32;
    let mut italic_depth = 0u32;
    let mut strike_depth = 0u32;
    let mut link_depth = 0u32;
    let mut blockquote_depth = 0u32;
    let mut heading_level: Option<HeadingLevel> = None;
    let mut code_block: Option<String> = None;
    let mut list_stack: Vec<(bool, u64)> = Vec::new();
    let mut job = egui::text::LayoutJob::default();
    let mut job_has_content = false;

    fn flush_job(ui: &mut egui::Ui, job: &mut egui::text::LayoutJob, has_content: &mut bool) {
        if *has_content {
            job.wrap.max_width = ui.available_width().max(1.0);
            ui.label(job.clone());
            *job = egui::text::LayoutJob::default();
            *has_content = false;
        }
    }

    let mut events = Parser::new_ext(
        markdown,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    );

    while let Some(event) = events.next() {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    flush_job(ui, &mut job, &mut job_has_content);
                    ui.add_space(8.0);
                    heading_level = Some(level);
                }
                Tag::BlockQuote(_) => {
                    blockquote_depth += 1;
                    flush_job(ui, &mut job, &mut job_has_content);
                }
                Tag::CodeBlock(_) => {
                    flush_job(ui, &mut job, &mut job_has_content);
                    code_block = Some(String::new());
                }
                Tag::List(start) => {
                    flush_job(ui, &mut job, &mut job_has_content);
                    list_stack.push((start.is_some(), start.unwrap_or(1)));
                }
                Tag::Item => {
                    flush_job(ui, &mut job, &mut job_has_content);
                    let depth = list_stack.len().saturating_sub(1) as f32;
                    let prefix = match list_stack.last_mut() {
                        Some((true, n)) => {
                            let s = format!("{n}. ");
                            *n += 1;
                            s
                        }
                        _ => "•  ".to_string(),
                    };
                    job.append(
                        &prefix,
                        depth * 18.0,
                        egui::text::TextFormat {
                            font_id: egui::FontId::proportional(base_size),
                            color: text_color,
                            ..Default::default()
                        },
                    );
                    job_has_content = true;
                }
                Tag::Emphasis => italic_depth += 1,
                Tag::Strong => bold_depth += 1,
                Tag::Strikethrough => strike_depth += 1,
                Tag::Link { .. } => link_depth += 1,
                Tag::Table(_) => {
                    flush_job(ui, &mut job, &mut job_has_content);
                    draw_markdown_table(ui, &mut events, base_size, text_color, strong_color);
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Paragraph => {
                    flush_job(ui, &mut job, &mut job_has_content);
                    ui.add_space(6.0);
                }
                TagEnd::Heading(_) => {
                    flush_job(ui, &mut job, &mut job_has_content);
                    ui.add_space(4.0);
                    heading_level = None;
                }
                TagEnd::BlockQuote(_) => {
                    blockquote_depth = blockquote_depth.saturating_sub(1);
                    flush_job(ui, &mut job, &mut job_has_content);
                    ui.add_space(4.0);
                }
                TagEnd::CodeBlock => {
                    if let Some(code) = code_block.take() {
                        egui::Frame::NONE
                            .fill(code_bg)
                            .inner_margin(egui::Margin::same(8))
                            .corner_radius(4)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(
                                            code.trim_end_matches('\n').to_string(),
                                        )
                                        .monospace()
                                        .size(base_size),
                                    )
                                    .wrap(),
                                );
                            });
                        ui.add_space(6.0);
                    }
                }
                TagEnd::List(_) => {
                    list_stack.pop();
                    if list_stack.is_empty() {
                        ui.add_space(4.0);
                    }
                }
                TagEnd::Item => flush_job(ui, &mut job, &mut job_has_content),
                TagEnd::Emphasis => italic_depth = italic_depth.saturating_sub(1),
                TagEnd::Strong => bold_depth = bold_depth.saturating_sub(1),
                TagEnd::Strikethrough => strike_depth = strike_depth.saturating_sub(1),
                TagEnd::Link => link_depth = link_depth.saturating_sub(1),
                _ => {}
            },
            Event::Text(text) => {
                if let Some(code) = code_block.as_mut() {
                    code.push_str(&text);
                } else {
                    let mut size = base_size;
                    let mut color = if blockquote_depth > 0 {
                        weak_color
                    } else {
                        text_color
                    };
                    if bold_depth > 0 {
                        color = strong_color;
                    }
                    if let Some(level) = heading_level {
                        size = match level {
                            HeadingLevel::H1 => base_size + 10.0,
                            HeadingLevel::H2 => base_size + 7.0,
                            HeadingLevel::H3 => base_size + 5.0,
                            HeadingLevel::H4 => base_size + 3.0,
                            HeadingLevel::H5 => base_size + 1.0,
                            HeadingLevel::H6 => base_size,
                        };
                        color = heading_color;
                    }
                    if link_depth > 0 {
                        color = palette.primary;
                    }
                    let strikethrough = if strike_depth > 0 {
                        egui::Stroke::new(1.0, color)
                    } else {
                        egui::Stroke::NONE
                    };
                    job.append(
                        &text,
                        0.0,
                        egui::text::TextFormat {
                            font_id: egui::FontId::proportional(size),
                            color,
                            italics: italic_depth > 0,
                            strikethrough,
                            ..Default::default()
                        },
                    );
                    job_has_content = true;
                }
            }
            Event::Code(text) => {
                job.append(
                    &text,
                    0.0,
                    egui::text::TextFormat {
                        font_id: mono_font.clone(),
                        color: text_color,
                        background: code_bg,
                        ..Default::default()
                    },
                );
                job_has_content = true;
            }
            Event::SoftBreak => {
                job.append(" ", 0.0, egui::text::TextFormat::default());
                job_has_content = true;
            }
            Event::HardBreak => flush_job(ui, &mut job, &mut job_has_content),
            Event::Rule => {
                flush_job(ui, &mut job, &mut job_has_content);
                ui.separator();
            }
            Event::TaskListMarker(checked) => {
                job.append(
                    if checked { "\u{2611} " } else { "\u{2610} " },
                    0.0,
                    egui::text::TextFormat {
                        font_id: egui::FontId::proportional(base_size),
                        color: text_color,
                        ..Default::default()
                    },
                );
                job_has_content = true;
            }
            _ => {}
        }
    }

    flush_job(ui, &mut job, &mut job_has_content);
}

/// Renders a Markdown table (already-consumed `Tag::Table` start event) as an
/// `egui::Grid`, consuming events directly from `events` up through its
/// matching `TagEnd::Table`. Cell content is flattened to plain text (inline
/// formatting within cells, e.g. bold headers, is dropped) to keep this
/// simple.
fn draw_markdown_table(
    ui: &mut egui::Ui,
    events: &mut pulldown_cmark::Parser,
    base_size: f32,
    text_color: egui::Color32,
    strong_color: egui::Color32,
) {
    use pulldown_cmark::{Event, Tag, TagEnd};

    egui::Grid::new(ui.id().with("markdown_table"))
        .striped(true)
        .spacing(egui::vec2(16.0, 4.0))
        .show(ui, |ui| {
            let mut in_head = false;
            let mut cell_text = String::new();

            for event in events.by_ref() {
                match event {
                    Event::Start(Tag::TableHead) => in_head = true,
                    Event::End(TagEnd::TableHead) => {
                        in_head = false;
                        ui.end_row();
                    }
                    Event::Start(Tag::TableCell) => cell_text.clear(),
                    Event::End(TagEnd::TableCell) => {
                        let color = if in_head { strong_color } else { text_color };
                        ui.label(
                            egui::RichText::new(cell_text.trim().to_string())
                                .size(base_size)
                                .color(color),
                        );
                    }
                    Event::End(TagEnd::TableRow) => ui.end_row(),
                    Event::Text(t) | Event::Code(t) => cell_text.push_str(&t),
                    Event::End(TagEnd::Table) => break,
                    _ => {}
                }
            }
        });

    ui.add_space(6.0);
}

fn draw_preview_details(
    ui: &mut egui::Ui,
    i18n: &I18n,
    preview_service: &mut PreviewService,
    path: &Path,
) {
    let metadata = std::fs::metadata(path).ok();

    egui::Grid::new("preview_pane_details_grid")
        .num_columns(2)
        .spacing(egui::vec2(12.0, 6.0))
        .show(ui, |ui| {
            if let Some(metadata) = &metadata {
                ui.weak(i18n.tr("preview_details_size"));
                ui.label(format_file_size(metadata.len()));
                ui.end_row();
            }

            if let Some(PreviewPayload::Image { size, .. }) = preview_service.get(path) {
                ui.weak(i18n.tr("preview_details_dimensions"));
                ui.label(format!("{} x {}", size[0], size[1]));
                ui.end_row();
            }

            if let Some(created) = metadata.as_ref().and_then(|m| m.created().ok()) {
                ui.weak(i18n.tr("preview_details_created"));
                ui.label(format_system_time(created));
                ui.end_row();
            }

            if let Some(modified) = metadata.as_ref().and_then(|m| m.modified().ok()) {
                ui.weak(i18n.tr("preview_details_modified"));
                ui.label(format_system_time(modified));
                ui.end_row();
            }
        });

    if metadata.is_none() {
        ui.add_space(8.0);
        ui.weak(i18n.tr("preview_details_unavailable"));
    }
}

fn format_file_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{size:.1} {}", UNITS[unit_idx])
    }
}

fn format_system_time(time: std::time::SystemTime) -> String {
    let dt: DateTime<Local> = time.into();
    dt.format("%Y-%m-%d %H:%M").to_string()
}

fn format_video_time(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}
