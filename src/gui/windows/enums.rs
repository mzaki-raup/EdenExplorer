#[derive(Clone, Debug)]
pub enum ThemeCustomizerAction {
    ThemeUpdated(crate::gui::theme::ThemeMode),
    ResetToDefaults(crate::gui::theme::ThemeMode),
    ExportTheme(crate::gui::theme::ThemeMode),
    ImportTheme(crate::gui::theme::ThemeMode),
}

#[derive(Clone, Debug)]
pub enum SettingsAction {
    ResetToDefaults,
    ResetFavourites,
    ApplySettings,
    ExportSettings,
    ImportSettings,
    /// Produced by the Settings page's Appearance category (which embeds the
    /// same theme editor that used to be its own floating window) - handled
    /// identically to how that window's actions always were.
    ThemeCustomizer(ThemeCustomizerAction),
}
