//! More sidebar places: cloud storage folders (OneDrive, Dropbox, Google
//! Drive, iCloud Drive, Box), WSL distributions, and mounted disc images.
//! Found from environment variables, the registry and the apps' own config
//! files (nothing is started), and cached, since the sidebar is drawn every
//! frame.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudKind {
    OneDrive,
    Dropbox,
    GoogleDrive,
    ICloud,
    Box,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CloudFolder {
    pub kind: CloudKind,
    pub name: String,
    pub path: PathBuf,
}

/// How long the lists are kept before being looked up again.
const REFRESH: Duration = Duration::from_secs(60);

fn read_reg_string(
    root: windows::Win32::System::Registry::HKEY,
    sub: &str,
    value: &str,
) -> Option<String> {
    use windows::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};
    use windows::core::HSTRING;
    let mut buf = vec![0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            root,
            &HSTRING::from(sub),
            &HSTRING::from(value),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut len),
        )
    };
    if status.is_err() {
        return None;
    }
    let chars = (len as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..chars.min(buf.len())])).filter(|s| !s.is_empty())
}

/// Subkey names of `HKCU\<sub>`.
fn reg_subkeys(sub: &str) -> Vec<String> {
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW,
    };
    use windows::core::HSTRING;
    let mut key = HKEY::default();
    if unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(sub),
            None,
            KEY_READ,
            &mut key,
        )
    }
    .is_err()
    {
        return Vec::new();
    }
    let mut out = Vec::new();
    for index in 0.. {
        let mut name = [0u16; 256];
        let mut name_len = name.len() as u32;
        let status = unsafe {
            RegEnumKeyExW(
                key,
                index,
                Some(windows::core::PWSTR(name.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            )
        };
        if status.is_err() {
            break;
        }
        out.push(String::from_utf16_lossy(&name[..name_len as usize]));
    }
    unsafe {
        let _ = RegCloseKey(key);
    }
    out
}

/// Dropbox's folders, from its `info.json` (`personal`, `business`).
fn dropbox_folders(info_json: &str) -> Vec<(String, PathBuf)> {
    let Ok(info) = serde_json::from_str::<serde_json::Value>(info_json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (kind, label) in [("personal", "Dropbox"), ("business", "Dropbox (Business)")] {
        if let Some(path) = info
            .get(kind)
            .and_then(|v| v.get("path"))
            .and_then(|p| p.as_str())
        {
            out.push((label.to_string(), PathBuf::from(path)));
        }
    }
    out
}

fn find_cloud_folders() -> Vec<CloudFolder> {
    use windows::Win32::System::Registry::HKEY_CURRENT_USER;
    let mut found: Vec<CloudFolder> = Vec::new();
    let mut add = |kind, name: String, path: PathBuf| {
        if path.is_dir() && !found.iter().any(|f| f.path == path) {
            found.push(CloudFolder { kind, name, path });
        }
    };

    // OneDrive: every signed-in account (personal and work/school).
    const ONEDRIVE: &str = r"Software\Microsoft\OneDrive\Accounts";
    for account in reg_subkeys(ONEDRIVE) {
        let sub = format!(r"{ONEDRIVE}\{account}");
        if let Some(folder) = read_reg_string(HKEY_CURRENT_USER, &sub, "UserFolder") {
            let name = if account.eq_ignore_ascii_case("Personal") {
                "OneDrive".to_string()
            } else {
                read_reg_string(HKEY_CURRENT_USER, &sub, "DisplayName")
                    .map(|org| format!("OneDrive - {org}"))
                    .unwrap_or_else(|| "OneDrive (Work)".into())
            };
            add(CloudKind::OneDrive, name, PathBuf::from(folder));
        }
    }
    for var in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Ok(folder) = std::env::var(var) {
            let path = PathBuf::from(&folder);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "OneDrive".into());
            add(CloudKind::OneDrive, name, path);
        }
    }

    // Dropbox.
    for var in ["APPDATA", "LOCALAPPDATA"] {
        if let Ok(base) = std::env::var(var)
            && let Ok(json) =
                std::fs::read_to_string(Path::new(&base).join("Dropbox").join("info.json"))
        {
            for (name, path) in dropbox_folders(&json) {
                add(CloudKind::Dropbox, name, path);
            }
        }
    }

    // Google Drive for desktop shows up as a drive labelled "Google Drive"
    // (with My Drive inside); older versions used a folder in the profile.
    for drive in crate::core::drives::get_drive_infos() {
        if drive.display.to_lowercase().contains("google drive") {
            let my_drive = drive.path.join("My Drive");
            let path = if my_drive.is_dir() {
                my_drive
            } else {
                drive.path.clone()
            };
            add(CloudKind::GoogleDrive, "Google Drive".into(), path);
        }
    }
    if let Some(home) = dirs::home_dir() {
        add(
            CloudKind::GoogleDrive,
            "Google Drive".into(),
            home.join("Google Drive"),
        );
        add(
            CloudKind::ICloud,
            "iCloud Drive".into(),
            home.join("iCloudDrive"),
        );
        add(CloudKind::Box, "Box".into(), home.join("Box"));
    }
    found
}

/// A list found in the background: lookups can touch slow or virtual
/// drives, so the sidebar never waits on them (it shows the last list
/// until a fresh one is in, then repaints).
struct Cached<T> {
    at: Option<Instant>,
    value: Option<T>,
    busy: bool,
}

impl<T> Cached<T> {
    const fn new() -> Self {
        Self {
            at: None,
            value: None,
            busy: false,
        }
    }
}

static CLOUD: Mutex<Cached<Vec<CloudFolder>>> = Mutex::new(Cached::new());
static WSL: Mutex<Cached<Vec<String>>> = Mutex::new(Cached::new());

/// Starts a background lookup if the list is due for one.
fn refresh<T: Send + 'static>(slot: &'static Mutex<Cached<T>>, find: fn() -> T) {
    let mut cache = slot.lock().unwrap_or_else(|e| e.into_inner());
    let stale = cache.at.is_none_or(|at| at.elapsed() >= REFRESH);
    if stale && !cache.busy {
        cache.busy = true;
        std::thread::spawn(move || {
            let value = find();
            let mut cache = slot.lock().unwrap_or_else(|e| e.into_inner());
            cache.value = Some(value);
            cache.at = Some(Instant::now());
            cache.busy = false;
            drop(cache);
            crate::gui::windows::windowsoverrides::request_repaint();
        });
    }
}

fn cached<T: Clone + Default + Send + 'static>(
    slot: &'static Mutex<Cached<T>>,
    find: fn() -> T,
) -> T {
    refresh(slot, find);
    let cache = slot.lock().unwrap_or_else(|e| e.into_inner());
    cache.value.clone().unwrap_or_default()
}

/// Cloud storage folders on this PC.
pub fn cloud_folders() -> Vec<CloudFolder> {
    cached(&CLOUD, find_cloud_folders)
}

/// Installed WSL distributions (default first).
pub fn wsl_distributions() -> Vec<String> {
    cached(&WSL, crate::core::terminal_shells::wsl_distributions)
}

/// Where a WSL distribution's files are (`\\wsl$\Ubuntu`). Opening it
/// starts the distribution if it isn't running, as in Explorer.
pub fn wsl_path(distro: &str) -> PathBuf {
    PathBuf::from(format!(r"\\wsl$\{distro}"))
}

/// The WSL distribution and Linux path of a folder inside one
/// (`\\wsl$\Ubuntu\home\me` or `\\wsl.localhost\Ubuntu\home\me` ->
/// `("Ubuntu", "/home/me")`).
pub fn wsl_location(path: &Path) -> Option<(String, String)> {
    let s = path.to_string_lossy();
    let rest = s
        .strip_prefix(r"\\?\UNC\")
        .or_else(|| s.strip_prefix(r"\\"))
        .or_else(|| s.strip_prefix("//"))?;
    let mut parts = rest.split(['\\', '/']).filter(|p| !p.is_empty());
    let host = parts.next()?;
    if !host.eq_ignore_ascii_case("wsl$") && !host.eq_ignore_ascii_case("wsl.localhost") {
        return None;
    }
    let distro = parts.next()?.to_string();
    let linux = format!("/{}", parts.collect::<Vec<_>>().join("/"));
    Some((distro, linux))
}

static IMAGES: Mutex<Cached<Vec<PathBuf>>> = Mutex::new(Cached::new());

/// Whether the drive at `root` (`E:\`) is a mounted disc or disk image
/// (an ISO or VHD opened in Windows), by its storage bus type.
fn is_mounted_image_uncached(root: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{
        IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_DEVICE_DESCRIPTOR,
        STORAGE_PROPERTY_QUERY, StorageDeviceProperty,
    };
    use windows::core::PCWSTR;
    const BUS_TYPE_FILE_BACKED_VIRTUAL: i32 = 15;

    let s = root.to_string_lossy();
    let Some(letter) = s
        .chars()
        .next()
        .filter(|c| c.is_ascii_alphabetic() && s.get(1..2) == Some(":"))
    else {
        return false;
    };
    let device: Vec<u16> = std::ffi::OsStr::new(&format!(r"\\.\{letter}:"))
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        let Ok(handle) = CreateFileW(
            PCWSTR(device.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        ) else {
            return false;
        };
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageDeviceProperty,
            QueryType: PropertyStandardQuery,
            AdditionalParameters: [0; 1],
        };
        let mut buffer = vec![0u8; 1024];
        let mut returned = 0u32;
        let ok = DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&query as *const _ as *const _),
            std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(buffer.as_mut_ptr().cast()),
            buffer.len() as u32,
            Some(&mut returned),
            None,
        )
        .is_ok();
        let _ = CloseHandle(handle);
        ok && returned as usize >= std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>()
            && (*(buffer.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR))
                .BusType
                .0
                == BUS_TYPE_FILE_BACKED_VIRTUAL
    }
}

/// Drives that are mounted images (looked up in the background, at most
/// once a minute).
pub fn is_mounted_image(root: &Path) -> bool {
    // Starts a lookup when due; the answer is read in place (this is asked
    // for every drive, every frame).
    refresh(&IMAGES, find_mounted_images);
    IMAGES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .value
        .as_ref()
        .is_some_and(|images| images.iter().any(|p| p == root))
}

fn find_mounted_images() -> Vec<PathBuf> {
    crate::core::drives::get_drive_infos()
        .into_iter()
        .map(|d| d.path)
        .filter(|d| is_mounted_image_uncached(d))
        .collect()
}

/// Ejects a mounted image (Windows' own Eject, as in Explorer).
pub fn eject(root: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{PCWSTR, w};
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        ShellExecuteW(
            None,
            w!("eject"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
    IMAGES.lock().unwrap_or_else(|e| e.into_inner()).at = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_dropbox_info() {
        let json = r#"{"personal": {"path": "C:\\Users\\me\\Dropbox", "host": 1}, "business": {"path": "D:\\Dropbox (Acme)"}}"#;
        assert_eq!(
            dropbox_folders(json),
            vec![
                ("Dropbox".to_string(), PathBuf::from(r"C:\Users\me\Dropbox")),
                (
                    "Dropbox (Business)".to_string(),
                    PathBuf::from(r"D:\Dropbox (Acme)")
                ),
            ]
        );
        assert!(dropbox_folders("not json").is_empty());
        assert_eq!(wsl_path("Ubuntu"), PathBuf::from(r"\\wsl$\Ubuntu"));
    }

    #[test]
    fn finds_wsl_locations() {
        let loc = |p: &str| wsl_location(Path::new(p));
        assert_eq!(loc(r"\\wsl$\Ubuntu"), Some(("Ubuntu".into(), "/".into())));
        assert_eq!(loc(r"\\wsl$\Ubuntu\"), Some(("Ubuntu".into(), "/".into())));
        assert_eq!(
            loc(r"\\WSL.localhost\Debian\home\me"),
            Some(("Debian".into(), "/home/me".into()))
        );
        assert_eq!(loc(r"\\server\share\x"), None);
        assert_eq!(
            loc(r"\\?\UNC\wsl$\Ubuntu\etc"),
            Some(("Ubuntu".into(), "/etc".into()))
        );
        assert_eq!(loc(r"\\wsl$"), None);
        assert_eq!(loc(r"C:\Users"), None);
    }
}
