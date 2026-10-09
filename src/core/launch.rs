use crate::core::fs::{MY_PC_PATH, MY_RECYCLE_BIN_PATH};
use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::ffi::c_void;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use unic_langid::LanguageIdentifier;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Foundation::{GetLastError, HWND, LPARAM, WPARAM};
use windows::Win32::Globalization::GetUserDefaultUILanguage;
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, ReleaseMutex,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetWindowThreadProcessId, IsWindowVisible, SW_RESTORE, SendMessageW,
    SetForegroundWindow, ShowWindow, WM_COPYDATA,
};
use windows::core::{BOOL, PCWSTR, PWSTR};

#[derive(RustEmbed)]
#[folder = "locales/"]
struct LaunchLocalizations;

const INSTANCE_MUTEX: &str = "Local\\EdenExplorer.SingleInstance.v1";
/// The option `core::update::restart` passes to the copy it starts. Not
/// documented for users: it only means "the instance you are replacing is
/// still shutting down", which nothing but the updater can truthfully say.
pub const REPLACING_PREVIOUS: &str = "--replacing-previous";
/// How long a starting instance waits for an outgoing one to let go of the
/// mutex before giving up on a clean handover.
const HANDOVER_ATTEMPTS: u32 = 25;
const HANDOVER_RETRY: Duration = Duration::from_millis(200);
const WINDOW_TITLE: &str = "EdenExplorer";
const COPYDATA_ID: usize = 0x4544_454e;
static FORWARDED_PATHS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
const LANGUAGE_MAP: &[(u16, &str)] = &[
    // English
    (0x0409, "en-US"),
    (0x0809, "en-US"),
    (0x0C09, "en-US"),
    (0x1009, "en-US"),
    (0x1409, "en-US"),
    (0x1809, "en-US"),
    (0x1C09, "en-US"),
    (0x2009, "en-US"),
    (0x2409, "en-US"),
    (0x2809, "en-US"),
    (0x2C09, "en-US"),
    (0x3009, "en-US"),
    (0x3409, "en-US"),
    // Indonesian
    (0x0421, "id-ID"),
    // Japanese
    (0x0411, "ja-JP"),
    // Chinese
    (0x0404, "zh-TW"),
    (0x0804, "zh-CN"),
    (0x0C04, "zh-HK"),
];

pub struct InstanceGuard {
    handle: HANDLE,
}

#[derive(Debug, Default)]
pub struct LaunchOptions {
    pub paths: Vec<PathBuf>,
    pub new_window: bool,
    /// Started by a self-update to take over from the instance that is on
    /// its way out (see `REPLACING_PREVIOUS`).
    pub replacing_previous: bool,
}

#[derive(Debug)]
pub enum LaunchError {
    UnknownOption(String),
    InvalidDirectory(PathBuf),
    MutexCreate(String),
    WindowNotReady,
    WindowRejected,
}

impl LaunchError {
    pub fn message(&self) -> String {
        let i18n = LaunchI18n::new();

        match self {
            Self::UnknownOption(option) => {
                let mut args = FluentArgs::new();
                args.set("option", option.as_str());
                i18n.tr_args("launch-unknown-option", &args)
            }

            Self::InvalidDirectory(path) => {
                let mut args = FluentArgs::new();
                args.set("path", path.display().to_string());
                i18n.tr_args("launch-invalid-directory", &args)
            }

            Self::MutexCreate(error) => {
                let mut args = FluentArgs::new();
                args.set("error", error.as_str());
                i18n.tr_args("launch-mutex-create", &args)
            }

            Self::WindowNotReady => i18n.tr("launch-window-not-ready"),

            Self::WindowRejected => i18n.tr("launch-window-rejected"),
        }
    }
}

struct LaunchI18n {
    primary: FluentBundle<FluentResource>,
    fallback: FluentBundle<FluentResource>,
}

impl LaunchI18n {
    fn new() -> Self {
        let locale = system_locale();
        println!("Windows locale: {}", locale);

        let langid = LanguageIdentifier::from_str(&locale)
            .unwrap_or_else(|_| LanguageIdentifier::from_str("en-US").unwrap());

        let mut primary = FluentBundle::new(vec![langid]);
        primary.set_use_isolating(false);
        load_locale(&mut primary, &locale);

        let mut fallback = FluentBundle::new(vec![LanguageIdentifier::from_str("en-US").unwrap()]);
        fallback.set_use_isolating(false);
        load_locale(&mut fallback, "en-US");

        Self { primary, fallback }
    }

    fn format_bundle(
        bundle: &FluentBundle<FluentResource>,
        key: &str,
        args: Option<&FluentArgs>,
    ) -> Option<String> {
        let message = bundle.get_message(key)?;
        let pattern = message.value()?;

        let mut errors = Vec::new();

        Some(
            bundle
                .format_pattern(pattern, args, &mut errors)
                .to_string(),
        )
    }

    fn tr(&self, key: &str) -> String {
        Self::format_bundle(&self.primary, key, None)
            .or_else(|| Self::format_bundle(&self.fallback, key, None))
            .unwrap_or_else(|| key.to_string())
    }

    fn tr_args(&self, key: &str, args: &FluentArgs) -> String {
        Self::format_bundle(&self.primary, key, Some(args))
            .or_else(|| Self::format_bundle(&self.fallback, key, Some(args)))
            .unwrap_or_else(|| key.to_string())
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
pub fn system_locale() -> String {
    let langid = unsafe { GetUserDefaultUILanguage() };

    LANGUAGE_MAP
        .iter()
        .find(|(id, _)| *id == langid)
        .map(|(_, locale)| (*locale).to_string())
        .unwrap_or_else(|| "en-US".to_string())
}

pub fn parse_args<I, S>(args: I) -> Result<LaunchOptions, LaunchError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut options = LaunchOptions::default();
    let mut after_double_dash = false;

    for raw in args.into_iter().skip(1) {
        let arg = raw.into();
        if !after_double_dash && arg == "--" {
            after_double_dash = true;
        } else if !after_double_dash && arg == "--new-window" {
            options.new_window = true;
        } else if !after_double_dash && arg == REPLACING_PREVIOUS {
            options.replacing_previous = true;
        } else if !after_double_dash && arg.starts_with('-') {
            return Err(LaunchError::UnknownOption(arg));
        } else if !arg.is_empty() {
            options.paths.push(normalize_path(PathBuf::from(arg)));
        }
    }

    Ok(options)
}

pub fn existing_directories(options: &LaunchOptions) -> Result<Vec<PathBuf>, LaunchError> {
    let mut paths = Vec::with_capacity(options.paths.len());

    for path in &options.paths {
        // A zip/7z/tar (or a folder inside one) opens like a folder too.
        if !is_virtual_path(path) && !path.is_dir() && crate::core::archive_view::split(path).is_none() {
            return Err(LaunchError::InvalidDirectory(path.clone()));
        }

        paths.push(path.clone());
    }

    Ok(paths)
}

/// Whether this process got to be the single instance.
enum Claim {
    Owned(InstanceGuard),
    AlreadyRunning,
}

/// Asks for the single-instance mutex once. `CreateMutexW` is the only
/// authority on who holds it - never a guess about whether some other
/// process still looks alive.
fn claim_instance() -> Result<Claim, LaunchError> {
    let name = wide(INSTANCE_MUTEX);
    let handle = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) }
        .map_err(|e| LaunchError::MutexCreate(e.to_string()))?;
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = CloseHandle(handle);
        }
        Ok(Claim::AlreadyRunning)
    } else {
        Ok(Claim::Owned(InstanceGuard { handle }))
    }
}

pub fn acquire_or_forward(paths: &[PathBuf]) -> Result<Option<InstanceGuard>, LaunchError> {
    if let Claim::Owned(guard) = claim_instance()? {
        return Ok(Some(guard));
    }
    let rejected = match forward_paths(paths) {
        Ok(()) => return Ok(None),
        Err(error) => error,
    };
    // Something holds the mutex but would not take the message. The usual
    // reason is that it is already shutting down - its window gone, or past
    // pumping messages, between being found and being written to. Refusing
    // to start then leaves the user with no window at all, so wait for the
    // mutex to come free and take over instead. If the other instance is
    // genuinely alive it keeps the mutex, ownership never comes, and the
    // original error is still what the user sees.
    for _ in 0..HANDOVER_ATTEMPTS {
        thread::sleep(HANDOVER_RETRY);
        if let Claim::Owned(guard) = claim_instance()? {
            return Ok(Some(guard));
        }
    }
    Err(rejected)
}

/// Takes the single-instance mutex for a copy started by a self-update,
/// waiting for the instance it replaces to let go of it.
///
/// This deliberately never forwards. `core::update::restart` spawns the new
/// copy and only *then* exits, so the new copy routinely starts while the
/// outgoing one still holds the mutex and still owns a window - and handing
/// our arguments to a program that is seconds from exiting ends with no
/// window at all and nothing on screen to explain why.
///
/// If the previous instance somehow never goes away, start anyway without
/// the guard: a second window is a much smaller problem than an update that
/// leaves the user with no program.
pub fn acquire_replacing_previous() -> Result<Option<InstanceGuard>, LaunchError> {
    for attempt in 0..HANDOVER_ATTEMPTS {
        if let Claim::Owned(guard) = claim_instance()? {
            return Ok(Some(guard));
        }
        if attempt + 1 < HANDOVER_ATTEMPTS {
            thread::sleep(HANDOVER_RETRY);
        }
    }
    Ok(None)
}

pub fn receive_copydata(lparam: LPARAM) -> bool {
    let data = unsafe { (lparam.0 as *const COPYDATASTRUCT).as_ref() };
    let Some(data) = data else { return false };
    if data.dwData != COPYDATA_ID || data.cbData == 0 || data.lpData.is_null() {
        return false;
    }

    let units = data.cbData as usize / size_of::<u16>();
    let words = unsafe { std::slice::from_raw_parts(data.lpData as *const u16, units) };
    let paths = words
        .split(|unit| *unit == 0)
        .take_while(|part| !part.is_empty())
        .map(String::from_utf16_lossy)
        .map(PathBuf::from)
        .filter(|path| is_virtual_path(path) || path.is_dir())
        .collect::<Vec<_>>();
    if let Ok(mut pending) = FORWARDED_PATHS.lock() {
        pending.extend(paths);
    }
    true
}

pub fn take_forwarded_paths() -> Vec<PathBuf> {
    FORWARDED_PATHS
        .lock()
        .map(|mut paths| std::mem::take(&mut *paths))
        .unwrap_or_default()
}

fn wide(value: &str) -> Vec<u16> {
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn load_locale(bundle: &mut FluentBundle<FluentResource>, locale: &str) {
    let path = format!("{}/main.ftl", locale);
    let file = match LaunchLocalizations::get(&path) {
        Some(file) => file,
        None => {
            println!("Missing locale file: {}", path);
            return;
        }
    };

    let source = match file.data {
        Cow::Borrowed(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => text.to_owned(),
            Err(e) => {
                println!("UTF-8 error: {:?}", e);
                return;
            }
        },
        Cow::Owned(bytes) => match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(e) => {
                println!("UTF-8 error: {:?}", e);
                return;
            }
        },
    };

    let resource = match FluentResource::try_new(source) {
        Ok(resource) => resource,
        Err((_, errors)) => {
            println!("Fluent parse errors: {:?}", errors);
            return;
        }
    };

    if let Err(errors) = bundle.add_resource(resource) {
        println!("add_resource errors: {:?}", errors);
    }
}

/// The running instance's main window.
///
/// Not by title alone. This app strips `WS_CAPTION` and paints its own chrome,
/// and the viewport is built `.with_title_shown(false)` - which left the window
/// text empty, so `FindWindowW` matched nothing and forwarding failed with
/// `WindowNotReady` no matter how long it retried. The text is now set
/// explicitly (`windowsoverrides::apply_window_override`), but an instance that
/// is still starting up will not have it yet, so fall back to looking for a
/// visible top-level window owned by another EdenExplorer process.
fn find_instance_window() -> Option<HWND> {
    let title = wide(WINDOW_TITLE);
    let hwnd = unsafe { FindWindowW(None, PCWSTR(title.as_ptr())) }.unwrap_or_default();
    if !hwnd.is_invalid() {
        return Some(hwnd);
    }

    let mut found: Option<HWND> = None;
    let _ = unsafe {
        EnumWindows(
            Some(enum_instance_window),
            LPARAM(&mut found as *mut Option<HWND> as isize),
        )
    };
    found
}

unsafe extern "system" fn enum_instance_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    const KEEP_GOING: BOOL = BOOL(1);
    const STOP: BOOL = BOOL(0);

    if !unsafe { IsWindowVisible(hwnd) }.as_bool() || !window_is_another_instance(hwnd) {
        return KEEP_GOING;
    }
    let out = unsafe { &mut *(lparam.0 as *mut Option<HWND>) };
    *out = Some(hwnd);
    STOP
}

/// Whether `hwnd` belongs to a *different* process running this same program.
fn window_is_another_instance(hwnd: HWND) -> bool {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 || pid == unsafe { GetCurrentProcessId() } {
        return false;
    }

    let Ok(process) =
        (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) })
    else {
        return false;
    };
    let mut buffer = [0u16; 512];
    let mut len = buffer.len() as u32;
    let queried = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        )
    };
    unsafe {
        let _ = CloseHandle(process);
    }
    if queried.is_err() {
        return false;
    }

    let path = String::from_utf16_lossy(&buffer[..len as usize]);
    Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("EdenExplorer.exe"))
}

fn forward_paths(paths: &[PathBuf]) -> Result<(), LaunchError> {
    let mut payload: Vec<u16> = Vec::new();
    for path in paths {
        payload.extend(path.as_os_str().encode_wide());
        payload.push(0);
    }
    payload.push(0);

    // The instance may still be starting up, so give it a moment to appear.
    let mut hwnd = HWND::default();
    for _ in 0..30 {
        if let Some(found) = find_instance_window() {
            hwnd = found;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if hwnd.is_invalid() {
        return Err(LaunchError::WindowNotReady);
    }

    let data = COPYDATASTRUCT {
        dwData: COPYDATA_ID,
        cbData: (payload.len() * size_of::<u16>()) as u32,
        lpData: payload.as_ptr() as *mut c_void,
    };
    let mut delivered = false;
    for _ in 0..20 {
        let result = unsafe {
            SendMessageW(
                hwnd,
                WM_COPYDATA,
                Some(WPARAM(0)),
                Some(LPARAM(&data as *const COPYDATASTRUCT as isize)),
            )
        };
        if result.0 != 0 {
            delivered = true;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if !delivered {
        return Err(LaunchError::WindowRejected);
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
    Ok(())
}

pub fn normalize_path(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();

    if value.eq_ignore_ascii_case(MY_PC_PATH)
        || value.eq_ignore_ascii_case("file:\\")
        || value.eq_ignore_ascii_case("shell:MyComputerFolder")
    {
        return PathBuf::from(MY_PC_PATH);
    }

    if value.eq_ignore_ascii_case(MY_RECYCLE_BIN_PATH)
        || value.eq_ignore_ascii_case("shell:RecycleBinFolder")
    {
        return PathBuf::from(MY_RECYCLE_BIN_PATH);
    }

    path
}

pub fn is_virtual_path(path: &Path) -> bool {
    matches!(
        path.to_string_lossy().as_ref(),
        MY_PC_PATH | MY_RECYCLE_BIN_PATH
    )
}

/// Whether `input` looks like a `shell:` moniker (e.g. `shell:ControlPanelFolder`)
/// or a raw CLSID path (`::{GUID}`) rather than an ordinary filesystem path -
/// the two forms Explorer's own address bar accepts beyond real paths/UNC.
pub fn is_shell_uri(input: &str) -> bool {
    let trimmed = input.trim();
    let shell_prefix = trimmed.get(..6).is_some_and(|p| p.eq_ignore_ascii_case("shell:"));
    shell_prefix || trimmed.starts_with("::{")
}

/// What a `shell:`/`::{GUID}` moniker resolved to via the shell namespace.
pub enum ShellUriResolution {
    /// Resolves to a real filesystem location - navigate there like any
    /// other path.
    FileSystemPath(PathBuf),
    /// A genuinely virtual shell folder (Control Panel, Printers, etc.) with
    /// no filesystem backing this app can render - hand off to Explorer/the
    /// shell itself via `ShellExecuteW` instead.
    Virtual,
}

/// Resolves a `shell:`/`::{GUID}` string through the Win32 shell namespace,
/// the same resolution Explorer's own address bar performs. Returns `None`
/// if `input` isn't shell-URI-shaped at all (use [`is_shell_uri`] to check
/// first, or just inspect the `None` case).
///
/// This reuses `SHCreateItemFromParsingName` - already used elsewhere in
/// this codebase for ordinary filesystem paths (`mainwindow_imp.rs`,
/// `core/utils/dialogs.rs`, `core/utils/thumbnails.rs`) - since it parses a
/// `shell:` moniker exactly the same way `SHParseDisplayName`/
/// `IShellFolder::ParseDisplayName` do; no separate API is needed.
pub fn resolve_shell_uri(input: &str) -> Option<ShellUriResolution> {
    if !is_shell_uri(input) {
        return None;
    }

    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH};
    use windows::core::HSTRING;

    unsafe {
        let item: IShellItem = match SHCreateItemFromParsingName(&HSTRING::from(input), None) {
            Ok(item) => item,
            Err(_) => return Some(ShellUriResolution::Virtual),
        };

        match item.GetDisplayName(SIGDN_FILESYSPATH) {
            Ok(name) => {
                let path = name
                    .to_string()
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| PathBuf::new());
                CoTaskMemFree(Some(name.0 as _));
                if path.as_os_str().is_empty() {
                    Some(ShellUriResolution::Virtual)
                } else {
                    Some(ShellUriResolution::FileSystemPath(path))
                }
            }
            Err(_) => Some(ShellUriResolution::Virtual),
        }
    }
}

/// Opens an ordinary http(s) URL in whatever the user's default browser is.
/// Separate from `launch_shell_uri_externally` below despite the identical
/// `ShellExecuteW` call - that one means "this is a shell namespace location
/// this app cannot render", which is a different intent from "show the user
/// this web page" (the release page behind an update offer, say).
pub fn open_url(url: &str) {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::HSTRING;

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return;
    }
    unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(url),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

/// Hands a virtual shell location off to the shell itself (Explorer opens
/// its own window for things like Control Panel/Printers) since this app has
/// no in-app rendering model for arbitrary virtual shell namespaces.
pub fn launch_shell_uri_externally(input: &str) {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::HSTRING;

    unsafe {
        ShellExecuteW(
            None,
            &HSTRING::from("open"),
            &HSTRING::from(input),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(not(windows))]
pub fn take_forwarded_paths() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_paths_and_new_window() {
        let options = parse_args([
            "EdenExplorer.exe",
            "--new-window",
            "C:\\Projects",
            "D:\\Archive",
        ])
        .unwrap();

        assert!(options.new_window);
        assert_eq!(
            options.paths,
            vec![PathBuf::from("C:\\Projects"), PathBuf::from("D:\\Archive")]
        );
    }

    #[test]
    fn the_self_update_restart_flag_is_recognised_and_is_not_a_path() {
        // `core::update::restart` passes exactly this, and a copy started
        // that way must wait for the instance it replaces rather than
        // forwarding to it - so the flag has to parse, and must never be
        // mistaken for a folder to open.
        let options = parse_args(["EdenExplorer.exe", REPLACING_PREVIOUS]).unwrap();
        assert!(options.replacing_previous);
        assert!(options.paths.is_empty());
        assert!(!options.new_window);
    }

    #[test]
    fn an_ordinary_launch_does_not_claim_to_be_replacing_anything() {
        let options = parse_args(["EdenExplorer.exe", "C:\\Projects"]).unwrap();
        assert!(!options.replacing_previous);
    }

    #[test]
    fn allows_dash_prefixed_paths_after_double_dash() {
        let options = parse_args(["EdenExplorer.exe", "--", "-folder"]).unwrap();
        assert_eq!(options.paths, vec![PathBuf::from("-folder")]);
    }

    #[test]
    fn rejects_unknown_options() {
        let error = parse_args(["EdenExplorer.exe", "--unknown"]).unwrap_err();
        match error {
            LaunchError::UnknownOption(option) => {
                assert_eq!(option, "--unknown");
            }
            _ => panic!("unexpected error"),
        }
    }

    #[test]
    fn startup_i18n_loads() {
        let i18n = LaunchI18n::new();

        let value = i18n.tr("launch-window-not-ready");

        assert_ne!(value, "");
    }

    #[test]
    fn normalizes_my_pc_aliases() {
        let aliases = [
            "::MY_PC::",
            "file:\\",
            "shell:MyComputerFolder",
            "SHELL:MYCOMPUTERFOLDER",
        ];

        for alias in aliases {
            let options = parse_args(["EdenExplorer.exe", alias]).unwrap();

            assert_eq!(
                options.paths,
                vec![PathBuf::from(MY_PC_PATH)],
                "failed to normalize {alias:?}"
            );
        }
    }

    #[test]
    fn normalizes_recycle_bin_aliases() {
        let aliases = [
            "::RECYCLE_BIN::",
            "shell:RecycleBinFolder",
            "SHELL:RECYCLEBINFOLDER",
        ];

        for alias in aliases {
            let options = parse_args(["EdenExplorer.exe", alias]).unwrap();

            assert_eq!(
                options.paths,
                vec![PathBuf::from(MY_RECYCLE_BIN_PATH)],
                "failed to normalize {alias:?}"
            );
        }
    }

    #[test]
    fn preserves_normal_directories() {
        let options = parse_args(["EdenExplorer.exe", "C:\\Projects", "D:\\Archive"]).unwrap();

        assert_eq!(
            options.paths,
            vec![PathBuf::from("C:\\Projects"), PathBuf::from("D:\\Archive"),]
        );
    }

    #[test]
    fn normalizes_virtual_paths_before_directory_validation() {
        let options =
            parse_args(["EdenExplorer.exe", "file:\\", "shell:RecycleBinFolder"]).unwrap();

        let paths = existing_directories(&options).unwrap();

        assert_eq!(
            paths,
            vec![
                PathBuf::from(MY_PC_PATH),
                PathBuf::from(MY_RECYCLE_BIN_PATH),
            ]
        );
    }
}
