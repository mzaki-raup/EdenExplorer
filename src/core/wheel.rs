//! How far one notch of the mouse wheel scrolls.
//!
//! Nothing between the mouse and this app consults Windows' own
//! "lines to scroll each time" setting: winit hands egui the raw notch
//! count (`WM_MOUSEWHEEL`'s delta over `WHEEL_DELTA`), and egui turns each
//! notch into a fixed number of points (`InputOptions::line_scroll_speed`).
//! So the distance is the app's to decide, and this module decides it - the
//! file list sets `line_scroll_speed` to its chosen number of *rows* per
//! notch, and the terminal converts the points it receives back into
//! notches and applies its own count.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Largest number of lines one notch may scroll, for both the setting and
/// the Windows value.
pub const MAX_LINES: u32 = 30;

/// Windows' "one screen at a time" choice has no row count; this stands in
/// for it, since a page's height is not known where the setting is applied.
const PAGE_LINES: u32 = 20;

/// The default when Windows cannot be asked (it has always been 3).
const WINDOWS_DEFAULT: u32 = 3;

/// The number of lines one notch scrolls, given the app's settings.
/// `0` lines never comes back: a wheel that does nothing is a broken wheel.
pub fn lines_per_notch(follow_windows: bool, lines: u32, windows_lines: u32) -> u32 {
    let chosen = if follow_windows { windows_lines } else { lines };
    chosen.clamp(1, MAX_LINES)
}

/// Windows' own lines-per-notch setting (Settings > Bluetooth & devices >
/// Mouse), re-read every few seconds so a change there takes effect without
/// a restart but the system call is not made on every frame.
pub fn windows_lines() -> u32 {
    static CACHE: Mutex<Option<(Instant, u32)>> = Mutex::new(None);
    const REFRESH: Duration = Duration::from_secs(5);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, lines)) = *cache {
        if at.elapsed() < REFRESH {
            return lines;
        }
    }
    let lines = query_windows_lines().unwrap_or(WINDOWS_DEFAULT);
    *cache = Some((Instant::now(), lines));
    lines
}

/// Maps the raw system value: `WHEEL_PAGESCROLL` (`u32::MAX`) means a page.
fn interpret_windows_value(raw: u32) -> u32 {
    if raw == u32::MAX {
        PAGE_LINES
    } else {
        raw.clamp(1, MAX_LINES)
    }
}

#[cfg(windows)]
fn query_windows_lines() -> Option<u32> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETWHEELSCROLLLINES, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut raw: u32 = 0;
    // SAFETY: SPI_GETWHEELSCROLLLINES writes one UINT to `pvParam`, which
    // points at a live local of exactly that size.
    unsafe {
        SystemParametersInfoW(
            SPI_GETWHEELSCROLLLINES,
            0,
            Some(&mut raw as *mut u32 as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .ok()?;
    }
    Some(interpret_windows_value(raw))
}

#[cfg(not(windows))]
fn query_windows_lines() -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_windows_when_asked_and_the_setting_otherwise() {
        assert_eq!(lines_per_notch(true, 12, 3), 3);
        assert_eq!(lines_per_notch(false, 12, 3), 12);
    }

    #[test]
    fn never_yields_a_wheel_that_does_nothing() {
        assert_eq!(lines_per_notch(false, 0, 3), 1);
        assert_eq!(lines_per_notch(true, 5, 0), 1);
    }

    #[test]
    fn caps_absurd_values_from_either_source() {
        assert_eq!(lines_per_notch(false, 999, 3), MAX_LINES);
        assert_eq!(lines_per_notch(true, 3, 999), MAX_LINES);
    }

    #[test]
    fn windows_page_scroll_becomes_a_fixed_count() {
        assert_eq!(interpret_windows_value(u32::MAX), PAGE_LINES);
        assert_eq!(interpret_windows_value(3), 3);
        assert_eq!(interpret_windows_value(0), 1);
    }

    #[test]
    fn asking_windows_gives_a_usable_count() {
        let lines = windows_lines();
        assert!((1..=MAX_LINES).contains(&lines));
        assert_eq!(windows_lines(), lines);
    }
}
