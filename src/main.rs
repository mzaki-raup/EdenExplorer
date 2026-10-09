#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod core;
mod gui;

use crate::core::indexer::{WindowSizeMode, load_window_position, load_windows_size_mode_on_start};
use crate::core::launch::{
    LaunchError, acquire_or_forward, acquire_replacing_previous, existing_directories, parse_args,
};
use crate::core::utils::fonts::apply_custom_font_definitions;
use crate::gui::windows::windowsoverrides::set_egui_ctx;
use eframe::{NativeOptions, egui};
use std::os::windows::ffi::OsStrExt;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, MB_ICONERROR, MB_OK, MessageBoxW, SM_CXSCREEN, SM_CXVIRTUALSCREEN,
    SM_CYSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};
use windows::core::PCWSTR;

fn main() -> eframe::Result<()> {
    // The elevated fast-scan helper (see `core::mft_helper`) is this same
    // exe started with a special argument; it never opens a window.
    if let Some(code) = crate::core::mft_helper::run_helper_from_args() {
        std::process::exit(code);
    }
    // Files opened from inside archives last session (in the background:
    // there may be many).
    std::thread::spawn(crate::core::archive_view::clean_temp);
    // Remote files downloaded to open or preview last session.
    std::thread::spawn(crate::core::remote::clean_temp);

    let launch_options = match parse_args(std::env::args()) {
        Ok(options) => options,
        Err(error) => {
            show_launch_error(&error);
            return Ok(());
        }
    };
    let launch_paths = match existing_directories(&launch_options) {
        Ok(paths) => paths,
        Err(error) => {
            show_launch_error(&error);
            return Ok(());
        }
    };

    // Not gated on having paths: an instance launched the ordinary way (no
    // arguments at all - Start menu, taskbar, a shortcut) has to register
    // itself too, or a later `EdenExplorer.exe <folder>` finds no instance to
    // forward to and starts a second window instead of opening a tab.
    let _instance_guard = if launch_options.new_window {
        None
    } else if launch_options.replacing_previous {
        // Started by a self-update: wait for the instance being replaced
        // rather than forwarding to it.
        match acquire_replacing_previous() {
            Ok(guard) => guard,
            Err(error) => {
                show_launch_error(&error);
                return Ok(());
            }
        }
    } else {
        match acquire_or_forward(&launch_paths) {
            Ok(Some(guard)) => Some(guard),
            Ok(None) => return Ok(()),
            Err(error) => {
                show_launch_error(&error);
                return Ok(());
            }
        }
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    let icon = load_icon();
    let window_size_mode = load_windows_size_mode_on_start();

    // The primary monitor - what "full screen" means, and what a fresh
    // install centres itself on.
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) } as f32;
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) } as f32;

    // The desktop across *every* monitor. The origin can be negative, for a
    // monitor placed left of or above the primary one.
    let virtual_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) } as f32;
    let virtual_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) } as f32;
    let virtual_w = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) } as f32;
    let virtual_h = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) } as f32;

    let window_size = match window_size_mode {
        WindowSizeMode::FullScreen => egui::Vec2::new(screen_w, screen_h),
        // Bounded by the whole desktop rather than the primary monitor: a
        // window restored onto a second screen must not be shrunk to the
        // primary's dimensions. `with_clamp_size_to_monitor_size(true)` below
        // still trims it to whichever monitor it actually lands on.
        WindowSizeMode::Custom { width, height } => {
            egui::Vec2::new(width.min(virtual_w), height.min(virtual_h))
        }
    };

    // Restore the window to wherever the user last left it, if we have a
    // saved position and it's still at least partly on a screen (guards
    // against a saved position from a monitor setup that's since changed) -
    // otherwise fall back to centering like a fresh install would.
    //
    // Checked against the whole desktop, not the primary monitor. The old
    // check compared `x` against the *primary* monitor's width, so every
    // position on a second monitor to the right of it failed and the window
    // silently jumped back to the primary - verified on a two-monitor setup,
    // where a saved x of 2100 (comfortably inside a 1920..3840 second screen)
    // was discarded and the window re-centred on the primary.
    let (pos_x, pos_y) = match load_window_position() {
        Some((x, y))
            if x + window_size.x > virtual_x
                && y + window_size.y > virtual_y
                && x < virtual_x + virtual_w
                && y < virtual_y + virtual_h =>
        {
            (x, y)
        }
        _ => (
            ((screen_w - window_size.x) * 0.5).max(0.0),
            ((screen_h - window_size.y) * 0.5).max(0.0),
        ),
    };

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(window_size)
            .with_position(egui::pos2(pos_x, pos_y))
            .with_icon(icon)
            .with_title_shown(false)
            .with_decorations(false)
            .with_clamp_size_to_monitor_size(true),
        ..Default::default()
    };

    eframe::run_native(
        "EdenExplorer",
        options,
        Box::new(|cc| {
            let mut fonts = egui::FontDefinitions::default();

            apply_custom_font_definitions(&mut fonts);

            cc.egui_ctx.set_fonts(fonts);
            set_egui_ctx(&cc.egui_ctx);

            Ok(Box::new(gui::MainWindow::new_with_paths(
                launch_paths.clone(),
            )))
        }),
    )
}

fn show_launch_error(error: &LaunchError) {
    let message = error.message();

    #[cfg(windows)]
    {
        let text: Vec<u16> = std::ffi::OsStr::new(&message)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let title: Vec<u16> = std::ffi::OsStr::new("EdenExplorer")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let _ = MessageBoxW(
                None,
                PCWSTR(text.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }

    #[cfg(not(windows))]
    eprintln!("{message}");
}

static ICON_BYTES: &[u8] = include_bytes!("assets/icon.ico");

fn load_icon() -> egui::IconData {
    match image::load_from_memory(ICON_BYTES) {
        Ok(img) => {
            let rgba = img.into_rgba8();
            let (width, height) = rgba.dimensions();
            egui::IconData {
                rgba: rgba.into_raw(),
                width,
                height,
            }
        }
        Err(_) => egui::IconData {
            rgba: vec![0u8; 64 * 64 * 4],
            width: 64,
            height: 64,
        },
    }
}
