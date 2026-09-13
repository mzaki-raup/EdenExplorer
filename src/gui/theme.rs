use crate::core::utils::fonts::{apply_custom_font_definitions, load_font_data};
use eframe::egui::{Color32, CornerRadius};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::{LazyLock, RwLock};

pub const THEME_VERSION: u32 = 8;

fn default_itemviewer_row_height() -> f32 {
    16.0
}

fn default_sidebar_icon_size() -> f32 {
    20.0
}

fn default_tab_icon_size() -> f32 {
    12.0
}

fn default_text_size() -> f32 {
    11.0
}

fn default_font_name() -> String {
    "Segoe UI".to_string()
}

fn default_mono_font_name() -> String {
    "Hack".to_string()
}

// Fallbacks used only when loading a settings file saved before these fields
// existed - `regenerate_base_derived_colors` recomputes accurate values the
// next time the accent color changes, these just need to look reasonable in
// the meantime (they match the dark theme's default derivation).
fn default_icon_color_dark() -> Color32 {
    Color32::WHITE
}

fn default_sidebar_bg_color() -> Color32 {
    Color32::from_rgb(24, 27, 32)
}

fn default_tab_inactive_bg_color() -> Color32 {
    Color32::from_rgb(28, 32, 38)
}

// Fallbacks for colors that were previously hardcoded per-mode literals in
// `apply_theme` rather than real fields - matches the dark theme's own
// values (same "close enough until the user edits it, or the mode's own
// default palette re-initializes it" reasoning as the fallbacks above).
fn default_faint_bg_color() -> Color32 {
    Color32::from_rgb(26, 30, 36)
}

fn default_input_field_bg() -> Color32 {
    Color32::from_rgb(30, 34, 40)
}

fn default_widget_text_inactive() -> Color32 {
    Color32::from_rgb(220, 226, 232)
}

fn default_widget_text_hovered() -> Color32 {
    Color32::from_rgb(245, 240, 255)
}

fn default_widget_text_active() -> Color32 {
    Color32::from_rgb(255, 250, 255)
}

fn default_selection_text_color() -> Color32 {
    Color32::WHITE
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum ThemeMode {
    Light,
    Dark,
}

impl Default for ThemeMode {
    fn default() -> Self {
        ThemeMode::Dark
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemePalette {
    // 🔤 Typography
    #[serde(default = "default_text_size")]
    pub text_size: f32,
    pub tooltip_text_size: f32,
    #[serde(default = "default_sidebar_icon_size")]
    pub sidebar_icon_size: f32,
    #[serde(default = "default_tab_icon_size")]
    pub tab_icon_size: f32,
    #[serde(default = "default_font_name")]
    pub font_name: String,
    #[serde(default = "default_mono_font_name")]
    pub mono_font_name: String,

    // 🎯 Brand / primary colors
    pub primary: Color32,
    pub primary_hover: Color32,
    pub primary_active: Color32,
    pub text_normal: Color32,
    pub text_header_section: Color32,

    // 🧱 Application surfaces
    pub application_bg_color: Color32,
    pub modal_background_effect_color: Color32,

    // 🎯 Icons & text
    #[serde(default = "default_icon_color_dark")]
    pub icon_color: Color32,
    pub icon_windows: Color32,
    pub icon_colored_hover: Color32,
    #[serde(default = "default_icon_color_dark")]
    pub toolbar_icon_color: Color32,
    pub item_viewer_row_text_selected: Color32,
    pub tooltip_text_color: Color32,
    pub tab_text_selected: Color32,

    // 🎯 Rows / list items
    pub row_selected_bg: Color32,
    pub row_bg: Color32,
    #[serde(default = "default_itemviewer_row_height")]
    pub row_height: f32,
    pub sidebar_item_spacing_y: f32,

    // 🧱 Panel surfaces (sidebar/tabs get their own accent-tinted surface,
    // distinct from the main explorer background)
    #[serde(default = "default_sidebar_bg_color")]
    pub sidebar_bg_color: Color32,
    #[serde(default = "default_tab_inactive_bg_color")]
    pub tab_inactive_bg_color: Color32,

    // 🎯 Handles / borders
    pub resize_handle: Color32,
    pub borders_default: Color32,
    pub borders_active: Color32,

    // 🎯 Tab button colors
    pub tab_close_hover: Color32,
    pub tab_close_active: Color32,
    pub tab_close_normal: Color32,
    pub tab_add_hover: Color32,
    pub pinned_tab_color: Color32,

    // 🎯 Drive usage colors
    pub drive_usage_critical: Color32,
    pub drive_usage_warning: Color32,
    pub drive_usage_normal: Color32,
    pub drive_usage_background: Color32,
    pub drive_usage_text: Color32,

    // ☑️ Checkbox
    pub checkbox_bg_default: Color32,
    pub checkbox_checkmark_color: Color32,
    pub checkbox_bg_hover: Color32,
    pub checkbox_bg_active: Color32,

    // 🔘 Buttons
    pub button_background: Color32,
    pub button_stroke: Color32,
    pub button_favorite_fill: Color32,
    pub button_seperator_handle_fill: Color32,

    // 🎯 Corner radius values
    pub small_radius: u8,
    pub medium_radius: u8,
    pub large_radius: u8,
    pub tab_active_radius: CornerRadius,
    pub tab_inactive_radius: CornerRadius,
    pub tab_button_radius: CornerRadius,

    // 🎯 Widget surfaces/text that used to be hardcoded per-mode literals in
    // `apply_theme` - promoted to real fields so a full re-theme can reach
    // them too. Defaults below match each mode's previous hardcoded value,
    // so existing users see zero visual change until they touch a picker.
    #[serde(default = "default_faint_bg_color")]
    pub faint_bg_color: Color32,
    /// Shared by a `TextEdit`'s own background (`extreme_bg_color`) and
    /// every other input widget's background (`widgets.inactive.bg_fill`) -
    /// these were already kept equal to each other in `apply_theme`, so one
    /// field covers both rather than risking them drifting apart.
    #[serde(default = "default_input_field_bg")]
    pub input_field_bg: Color32,
    #[serde(default = "default_widget_text_inactive")]
    pub widget_text_inactive: Color32,
    #[serde(default = "default_widget_text_hovered")]
    pub widget_text_hovered: Color32,
    #[serde(default = "default_widget_text_active")]
    pub widget_text_active: Color32,
    /// Text color repainted over a text selection highlight - kept separate
    /// from `text_normal` since it needs to contrast against
    /// `style.visuals.selection.bg_fill`, not the page background.
    #[serde(default = "default_selection_text_color")]
    pub selection_text_color: Color32,
}

// 🎯 Single base color (your purple)
fn base_color() -> Color32 {
    Color32::from_rgb(110, 85, 160)
}

// 🌙 Dark theme palette (defaults)
pub static DEFAULT_PALETTE_DARK: LazyLock<ThemePalette> = LazyLock::new(|| {
    let base = base_color();
    let mut palette = ThemePalette {
        // 🔤 Typography
        text_size: 12.0,
        tooltip_text_size: 13.0,
        sidebar_icon_size: 20.0,
        tab_icon_size: 12.0,
        font_name: default_font_name(),
        mono_font_name: default_mono_font_name(),

        // 🎯 Brand / primary colors
        primary: base,
        primary_hover: Color32::from_rgba_unmultiplied(95, 75, 135, 128),
        primary_active: Color32::from_rgb(70, 55, 110),
        text_normal: Color32::from_rgb(160, 170, 180),
        text_header_section: Color32::WHITE,

        // 🧱 Application surfaces
        application_bg_color: Color32::from_rgb(20, 22, 26),
        modal_background_effect_color: Color32::from_black_alpha(180),

        // 🎯 Icons & text
        icon_color: Color32::WHITE,
        icon_windows: Color32::WHITE,
        icon_colored_hover: Color32::WHITE,
        toolbar_icon_color: Color32::WHITE,
        item_viewer_row_text_selected: Color32::WHITE,
        tooltip_text_color: Color32::from_rgb(160, 170, 180),
        tab_text_selected: Color32::WHITE,

        // 🎯 Rows / list items
        row_selected_bg: Color32::from_rgb(70, 78, 86),
        row_bg: Color32::from_rgb(40, 45, 50),
        row_height: default_itemviewer_row_height(),
        sidebar_item_spacing_y: 0.5,

        // 🧱 Panel surfaces
        sidebar_bg_color: Color32::from_rgb(24, 27, 32),
        tab_inactive_bg_color: Color32::from_rgb(28, 32, 38),

        // 🎯 Handles / borders
        resize_handle: Color32::from_rgb(160, 170, 180),
        borders_default: Color32::from_rgba_unmultiplied(95, 75, 135, 60),
        borders_active: base,

        // 🎯 Tab button colors
        tab_close_hover: Color32::from_rgb(200, 52, 52),
        tab_close_active: Color32::WHITE,
        tab_close_normal: Color32::from_rgb(160, 170, 180),
        tab_add_hover: Color32::from_rgb(54, 168, 82),
        pinned_tab_color: Color32::from_rgb(242, 201, 76),

        // 🎯 Drive usage colors
        drive_usage_critical: Color32::from_rgb(220, 60, 60),
        drive_usage_warning: Color32::from_rgb(245, 170, 60),
        drive_usage_normal: Color32::from_rgb(60, 190, 110),
        drive_usage_background: Color32::from_rgba_unmultiplied(160, 170, 180, 20),
        drive_usage_text: Color32::WHITE,

        // ☑️ Checkbox
        checkbox_bg_default: Color32::from_rgba_unmultiplied(160, 170, 180, 20),
        checkbox_checkmark_color: Color32::WHITE,
        checkbox_bg_hover: base,
        checkbox_bg_active: base,

        // 🔘 Buttons
        button_background: Color32::from_rgba_unmultiplied(160, 170, 180, 20),
        button_stroke: Color32::from_rgba_unmultiplied(160, 170, 180, 60),
        button_favorite_fill: Color32::from_rgb(242, 201, 76),
        button_seperator_handle_fill: Color32::from_gray(120),

        // 🎯 Corner radius values
        small_radius: 2,
        medium_radius: 4,
        large_radius: 6,
        tab_active_radius: CornerRadius {
            nw: 8,
            ne: 8,
            sw: 0,
            se: 0,
        },
        tab_inactive_radius: CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        },
        tab_button_radius: CornerRadius::same(4),

        // 🎯 Widget surfaces/text (formerly hardcoded in `apply_theme`)
        faint_bg_color: Color32::from_rgb(26, 30, 36),
        input_field_bg: Color32::from_rgb(30, 34, 40),
        widget_text_inactive: Color32::from_rgb(220, 226, 232),
        widget_text_hovered: Color32::from_rgb(245, 240, 255),
        widget_text_active: Color32::from_rgb(255, 250, 255),
        selection_text_color: Color32::from_rgb(255, 255, 255),
    };
    regenerate_base_derived_colors(&mut palette, true);
    palette
});

// ☀️ Light theme palette (defaults)
pub static DEFAULT_PALETTE_LIGHT: LazyLock<ThemePalette> = LazyLock::new(|| {
    let base = base_color();
    let mut palette = ThemePalette {
        // 🔤 Typography
        text_size: 12.0,
        tooltip_text_size: 13.0,
        sidebar_icon_size: 20.0,
        tab_icon_size: 12.0,
        font_name: default_font_name(),
        mono_font_name: default_mono_font_name(),

        // 🎯 Brand / primary colors
        primary: base,
        primary_hover: Color32::from_rgba_unmultiplied(110, 85, 160, 90),
        primary_active: Color32::from_rgb(140, 120, 200),
        text_normal: Color32::from_rgb(70, 78, 86),
        text_header_section: Color32::BLACK,

        // 🧱 Application surfaces
        application_bg_color: Color32::from_rgb(245, 245, 245),
        modal_background_effect_color: Color32::from_black_alpha(180),

        // 🎯 Icons & text
        icon_color: Color32::from_rgb(40, 40, 40),
        icon_windows: Color32::WHITE,
        icon_colored_hover: Color32::WHITE,
        toolbar_icon_color: Color32::from_rgb(40, 40, 40),
        item_viewer_row_text_selected: Color32::BLACK,
        tooltip_text_color: Color32::from_rgb(40, 40, 40),
        tab_text_selected: Color32::WHITE,

        // 🎯 Rows / list items
        row_selected_bg: Color32::from_rgb(70, 78, 86),
        row_bg: Color32::from_rgb(240, 245, 250),
        row_height: default_itemviewer_row_height(),
        sidebar_item_spacing_y: 0.5,

        // 🧱 Panel surfaces
        sidebar_bg_color: Color32::from_rgb(238, 239, 242),
        tab_inactive_bg_color: Color32::from_rgb(233, 235, 239),

        // 🎯 Handles / borders
        resize_handle: Color32::from_rgb(160, 170, 180),
        borders_default: Color32::from_rgba_unmultiplied(110, 85, 160, 40),
        borders_active: base,

        // 🎯 Tab button colors
        tab_close_hover: Color32::from_rgb(200, 52, 52),
        tab_close_active: Color32::WHITE,
        tab_close_normal: Color32::from_rgb(40, 40, 40),
        tab_add_hover: Color32::from_rgb(54, 168, 82),
        pinned_tab_color: Color32::from_rgb(242, 201, 76),

        // 🎯 Drive usage colors
        drive_usage_critical: Color32::from_rgb(200, 52, 52),
        drive_usage_warning: Color32::from_rgb(235, 155, 60),
        drive_usage_normal: Color32::from_rgb(54, 168, 82),
        drive_usage_background: Color32::from_rgba_unmultiplied(200, 210, 220, 120),
        drive_usage_text: Color32::from_rgb(70, 78, 86),

        // ☑️ Checkbox
        checkbox_bg_default: Color32::from_rgba_unmultiplied(160, 170, 180, 95),
        checkbox_checkmark_color: Color32::WHITE,
        checkbox_bg_hover: base,
        checkbox_bg_active: base,

        // 🔘 Buttons
        button_background: Color32::from_rgba_unmultiplied(160, 170, 180, 95),
        button_stroke: Color32::from_rgba_unmultiplied(160, 170, 180, 60),
        button_favorite_fill: Color32::from_rgb(242, 201, 76),
        button_seperator_handle_fill: Color32::from_gray(180),

        // 🎯 Corner radius values
        small_radius: 2,
        medium_radius: 4,
        large_radius: 6,
        tab_active_radius: CornerRadius {
            nw: 8,
            ne: 8,
            sw: 0,
            se: 0,
        },
        tab_inactive_radius: CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        },
        tab_button_radius: CornerRadius::same(4),

        // 🎯 Widget surfaces/text (formerly hardcoded in `apply_theme`)
        faint_bg_color: Color32::from_rgb(244, 246, 249),
        input_field_bg: Color32::from_rgb(247, 248, 250),
        widget_text_inactive: Color32::from_rgb(35, 41, 47),
        widget_text_hovered: Color32::from_rgb(25, 29, 33),
        widget_text_active: Color32::from_rgb(15, 18, 22),
        selection_text_color: Color32::from_rgb(15, 18, 22),
    };
    regenerate_base_derived_colors(&mut palette, false);
    palette
});

// 🎯 Runtime-editable palettes
static PALETTE_DARK: LazyLock<RwLock<ThemePalette>> =
    LazyLock::new(|| RwLock::new(DEFAULT_PALETTE_DARK.clone()));
static PALETTE_LIGHT: LazyLock<RwLock<ThemePalette>> =
    LazyLock::new(|| RwLock::new(DEFAULT_PALETTE_LIGHT.clone()));

// Usage:
pub fn get_palette(mode: ThemeMode) -> ThemePalette {
    match mode {
        ThemeMode::Dark => PALETTE_DARK
            .read()
            .map(|p| p.clone())
            .unwrap_or_else(|_| DEFAULT_PALETTE_DARK.clone()),
        ThemeMode::Light => PALETTE_LIGHT
            .read()
            .map(|p| p.clone())
            .unwrap_or_else(|_| DEFAULT_PALETTE_LIGHT.clone()),
    }
}

pub fn get_default_palette(mode: ThemeMode) -> ThemePalette {
    match mode {
        ThemeMode::Dark => DEFAULT_PALETTE_DARK.clone(),
        ThemeMode::Light => DEFAULT_PALETTE_LIGHT.clone(),
    }
}

pub fn set_palette(mode: ThemeMode, palette: ThemePalette) {
    let target = match mode {
        ThemeMode::Dark => &PALETTE_DARK,
        ThemeMode::Light => &PALETTE_LIGHT,
    };

    if let Ok(mut guard) = target.write() {
        *guard = palette;
    }
}

pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode) {
    let theme = match mode {
        ThemeMode::Dark => egui::Theme::Dark,
        ThemeMode::Light => egui::Theme::Light,
    };

    let mut style = (*ctx.style_of(theme)).clone();
    let palette = get_palette(mode);

    style.visuals = match mode {
        ThemeMode::Dark => egui::Visuals::dark(),
        ThemeMode::Light => egui::Visuals::light(),
    };

    // 📐 Layout / spacing
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.window_margin = egui::Margin::same(10);

    // 🔤 Typography
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::proportional(palette.text_size + 4.0),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::proportional(palette.text_size),
    );

    // 🔲 Shape
    style.visuals.window_corner_radius = CornerRadius::same(10);
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(6);

    style.visuals.panel_fill = palette.application_bg_color;
    style.visuals.faint_bg_color = palette.faint_bg_color;

    style.visuals.widgets.inactive.bg_fill = palette.input_field_bg;
    // `extreme_bg_color` is what a `TextEdit`'s own interior uses (as
    // opposed to `widgets.inactive.bg_fill`, which a `DragValue` or button
    // uses) - kept equal to `input_field_bg` so a text field doesn't read as
    // a completely different color than every other input in the same form.
    style.visuals.extreme_bg_color = palette.input_field_bg;

    // 🎯 PRIMARY SYSTEM
    style.visuals.widgets.hovered.bg_fill = palette.primary_hover;
    style.visuals.widgets.hovered.weak_bg_fill = palette.primary_hover;
    style.visuals.widgets.hovered.bg_stroke.color = palette.primary_hover;

    style.visuals.widgets.active.bg_fill = palette.primary_active;

    style.visuals.selection.bg_fill = match mode {
        ThemeMode::Dark => palette.primary_hover,
        ThemeMode::Light => palette.borders_default,
    };
    // Selected text is repainted in this color (not the field's normal text
    // color) - the highlight itself is a mid-tone accent shade, so this
    // needs to be a separate, deliberately high-contrast color rather than
    // reusing `primary_active`/the accent directly.
    style.visuals.selection.stroke.color = palette.selection_text_color;

    // 🔤 Text
    style.visuals.widgets.inactive.fg_stroke.color = palette.widget_text_inactive;
    style.visuals.widgets.hovered.fg_stroke.color = palette.widget_text_hovered;
    style.visuals.widgets.active.fg_stroke.color = palette.widget_text_active;
    style.visuals.widgets.noninteractive.fg_stroke.color = palette.text_normal;

    ctx.set_style_of(theme, style);
    ctx.set_theme(theme);
}

pub fn apply_checkbox_colors(ui: &mut egui::Ui, palette: &ThemePalette, checked: bool) {
    let visuals = &mut ui.visuals_mut().widgets;

    // Determine the background color depending on checked state
    let bg_fill = if checked {
        palette.checkbox_bg_active // "base" color when checked
    } else {
        palette.checkbox_bg_default
    };

    // Background fill
    visuals.inactive.bg_fill = bg_fill;
    visuals.hovered.bg_fill = if checked {
        palette.checkbox_bg_active
    } else {
        palette.checkbox_bg_hover
    };
    visuals.active.bg_fill = palette.checkbox_bg_active;

    // Border / stroke
    visuals.inactive.bg_stroke.color = bg_fill;
    visuals.hovered.bg_stroke.color = if checked {
        palette.checkbox_bg_active
    } else {
        palette.checkbox_bg_hover
    };
    visuals.active.bg_stroke.color = palette.checkbox_bg_active;

    // Checkmark color
    visuals.inactive.fg_stroke.color = palette.checkbox_checkmark_color;
    visuals.hovered.fg_stroke.color = palette.checkbox_checkmark_color;
    visuals.active.fg_stroke.color = palette.checkbox_checkmark_color;
}

/// Like `tint`, but for a translucent neutral (e.g. `button_background`'s
/// low-alpha gray) rather than an opaque surface color - tints the RGB the
/// same way, then reapplies `alpha` rather than the fully-opaque result
/// `tint` itself would produce.
fn tint_alpha(base_rgb: Color32, accent: Color32, t: f32, alpha: u8) -> Color32 {
    let tinted = tint(base_rgb, accent, t);
    Color32::from_rgba_unmultiplied(tinted.r(), tinted.g(), tinted.b(), alpha)
}

/// Blend `base` toward `accent` by `t` (0 = pure `base`, 1 = pure `accent`).
/// Used to give neutral surfaces/icons a subtle hint of the accent color
/// without losing contrast, rather than painting them the raw accent.
fn tint(base: Color32, accent: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |b: u8, a: u8| (b as f32 * (1.0 - t) + a as f32 * t).round() as u8;
    Color32::from_rgb(
        mix(base.r(), accent.r()),
        mix(base.g(), accent.g()),
        mix(base.b(), accent.b()),
    )
}

// 🎯 Regenerate all base-derived colors when primary (accent) color changes.
// Backgrounds get a subtle accent tint (`tint`); text/icons that need to
// stay legible against any accent are left as-is by callers - only the
// surfaces below, plus the pre-existing interactive-highlight colors, are
// accent-driven.
pub fn regenerate_base_derived_colors(palette: &mut ThemePalette, is_dark: bool) {
    let base = palette.primary;

    if is_dark {
        // Dark theme calculations
        palette.primary_hover = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 128);
        palette.primary_active = Color32::from_rgb(
            ((base.r() as u16 * 7) / 10) as u8, // 70% of red
            ((base.g() as u16 * 7) / 10) as u8, // 70% of green
            ((base.b() as u16 * 7) / 10) as u8, // 70% of blue
        );
        palette.borders_default = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 60);

        // Surfaces: a faint accent tint over each area's neutral dark tone,
        // kept distinct from one another so panels still read as separate.
        palette.application_bg_color = tint(Color32::from_rgb(20, 22, 26), base, 0.05);
        palette.sidebar_bg_color = tint(Color32::from_rgb(24, 27, 32), base, 0.08);
        palette.row_bg = tint(Color32::from_rgb(40, 45, 50), base, 0.05);
        palette.tab_inactive_bg_color = tint(Color32::from_rgb(28, 32, 38), base, 0.10);

        // Icons: white base, only slightly warmed toward the accent so they
        // stay legible against any accent hue; toolbar icons get a touch
        // more so the toolbar reads as "branded" at a glance.
        palette.icon_color = tint(Color32::WHITE, base, 0.15);
        palette.toolbar_icon_color = tint(Color32::WHITE, base, 0.30);

        let neutral = Color32::from_rgb(160, 170, 180);
        palette.button_background = tint_alpha(neutral, base, 0.16, 20);
        palette.button_stroke = tint_alpha(neutral, base, 0.28, 70);
    } else {
        // Light theme calculations
        palette.primary_hover = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 90);
        palette.primary_active = Color32::from_rgb(
            ((base.r() as u16 * 14) / 10).min(255) as u8, // 140% of red
            ((base.g() as u16 * 14) / 10).min(255) as u8, // 140% of green
            ((base.b() as u16 * 14) / 10).min(255) as u8, // 140% of blue
        );
        palette.borders_default = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 40);

        palette.application_bg_color = tint(Color32::from_rgb(245, 245, 245), base, 0.05);
        palette.sidebar_bg_color = tint(Color32::from_rgb(238, 239, 242), base, 0.08);
        palette.row_bg = tint(Color32::from_rgb(240, 245, 250), base, 0.05);
        palette.tab_inactive_bg_color = tint(Color32::from_rgb(233, 235, 239), base, 0.10);

        palette.icon_color = tint(Color32::from_rgb(40, 40, 40), base, 0.15);
        palette.toolbar_icon_color = tint(Color32::from_rgb(40, 40, 40), base, 0.30);

        let neutral = Color32::from_rgb(160, 170, 180);
        palette.button_background = tint_alpha(neutral, base, 0.16, 95);
        palette.button_stroke = tint_alpha(neutral, base, 0.28, 70);
    }

    // Common to both themes
    palette.borders_active = base;
    palette.checkbox_bg_hover = base;
    palette.checkbox_bg_active = base;
}

/// A named, color-only theme preset - the "bigger-grained" alternative to
/// hand-picking an accent + core colors one at a time. Deliberately small:
/// only the accent plus a couple of curated, purely-decorative fields
/// (the pinned-tab star, the favorite-star fill) that aren't already
/// recomputed by `regenerate_base_derived_colors`, so applying a preset
/// can't clobber semantic colors (drive-usage warning/critical, tab-close
/// hover, etc.) that need to keep meaning the same across every theme.
pub struct ThemePresetDef {
    pub name_key: &'static str,
    pub accent: Color32,
    pub pinned_tab_color: Color32,
    pub button_favorite_fill: Color32,
}

pub static PALETTE_PRESETS: &[ThemePresetDef] = &[
    ThemePresetDef {
        name_key: "theme_preset_default",
        accent: Color32::from_rgb(110, 85, 160),
        pinned_tab_color: Color32::from_rgb(242, 201, 76),
        button_favorite_fill: Color32::from_rgb(242, 201, 76),
    },
    ThemePresetDef {
        name_key: "theme_preset_midnight",
        accent: Color32::from_rgb(54, 90, 196),
        pinned_tab_color: Color32::from_rgb(176, 196, 255),
        button_favorite_fill: Color32::from_rgb(176, 196, 255),
    },
    ThemePresetDef {
        name_key: "theme_preset_ocean",
        accent: Color32::from_rgb(0, 150, 160),
        pinned_tab_color: Color32::from_rgb(224, 196, 120),
        button_favorite_fill: Color32::from_rgb(224, 196, 120),
    },
    ThemePresetDef {
        name_key: "theme_preset_forest",
        accent: Color32::from_rgb(46, 133, 64),
        pinned_tab_color: Color32::from_rgb(214, 170, 64),
        button_favorite_fill: Color32::from_rgb(214, 170, 64),
    },
    ThemePresetDef {
        name_key: "theme_preset_sunset",
        accent: Color32::from_rgb(214, 94, 54),
        pinned_tab_color: Color32::from_rgb(255, 178, 74),
        button_favorite_fill: Color32::from_rgb(255, 178, 74),
    },
    ThemePresetDef {
        name_key: "theme_preset_slate",
        accent: Color32::from_rgb(90, 98, 110),
        pinned_tab_color: Color32::from_rgb(176, 182, 190),
        button_favorite_fill: Color32::from_rgb(176, 182, 190),
    },
    ThemePresetDef {
        name_key: "theme_preset_crimson",
        accent: Color32::from_rgb(196, 43, 60),
        pinned_tab_color: Color32::from_rgb(255, 158, 122),
        button_favorite_fill: Color32::from_rgb(255, 158, 122),
    },
    ThemePresetDef {
        name_key: "theme_preset_magenta",
        accent: Color32::from_rgb(191, 0, 119),
        pinned_tab_color: Color32::from_rgb(255, 158, 204),
        button_favorite_fill: Color32::from_rgb(255, 158, 204),
    },
    ThemePresetDef {
        name_key: "theme_preset_violet",
        accent: Color32::from_rgb(139, 58, 201),
        pinned_tab_color: Color32::from_rgb(216, 180, 254),
        button_favorite_fill: Color32::from_rgb(216, 180, 254),
    },
    ThemePresetDef {
        name_key: "theme_preset_indigo",
        accent: Color32::from_rgb(63, 81, 181),
        pinned_tab_color: Color32::from_rgb(165, 175, 224),
        button_favorite_fill: Color32::from_rgb(165, 175, 224),
    },
    ThemePresetDef {
        name_key: "theme_preset_skyline",
        accent: Color32::from_rgb(0, 120, 215),
        pinned_tab_color: Color32::from_rgb(255, 214, 64),
        button_favorite_fill: Color32::from_rgb(255, 214, 64),
    },
];

/// Applies a named preset's accent + curated colors to `palette`, then
/// re-derives every accent-driven surface/icon color exactly like manually
/// editing the accent swatch would - a preset is just a bundle of manual
/// edits applied in one click, not a separate code path.
pub fn apply_theme_preset(palette: &mut ThemePalette, preset: &ThemePresetDef, is_dark: bool) {
    palette.primary = preset.accent;
    palette.pinned_tab_color = preset.pinned_tab_color;
    palette.button_favorite_fill = preset.button_favorite_fill;
    regenerate_base_derived_colors(palette, is_dark);
}

pub fn apply_font_to_context(ctx: &egui::Context, palette: &ThemePalette) {
    let mut fonts = egui::FontDefinitions::default();

    // Load proportional font
    if let Some(font_data) = load_font_data(&palette.font_name) {
        fonts.font_data.insert(
            "custom_proportional".to_string(),
            Arc::new(egui::FontData::from_owned(font_data)),
        );

        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "custom_proportional".to_string());
    }

    // Load monospace font
    if let Some(font_data) = load_font_data(&palette.mono_font_name) {
        fonts.font_data.insert(
            "custom_monospace".to_string(),
            Arc::new(egui::FontData::from_owned(font_data)),
        );

        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "custom_monospace".to_string());
    }

    apply_custom_font_definitions(&mut fonts);

    ctx.set_fonts(fonts);
    ctx.request_repaint();
}
