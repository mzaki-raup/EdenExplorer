//! Passwords (and SSH key passphrases) for remote locations, kept in
//! Windows Credential Manager (per user, encrypted by Windows) under
//! `EdenExplorer/remote/<id>` - never in EdenExplorer's own settings. They
//! show in Control Panel > Credential Manager > Windows Credentials. SSH host
//! keys trusted on first connect are kept in `remote_host_keys.json`.

use std::collections::HashMap;
use std::os::windows::ffi::OsStrExt;
use windows::Win32::Security::Credentials::{
    CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
    CredReadW, CredWriteW,
};
use windows::core::{PCWSTR, PWSTR};

fn target(id: u64) -> Vec<u16> {
    std::ffi::OsStr::new(&format!("EdenExplorer/remote/{id}"))
        .encode_wide()
        .chain(Some(0))
        .collect()
}

/// Stores `secret` for connection `id` (an empty one removes it).
pub fn store(id: u64, secret: &str) -> Result<(), String> {
    if secret.is_empty() {
        remove(id);
        return Ok(());
    }
    let mut name = target(id);
    let mut user: Vec<u16> = "EdenExplorer".encode_utf16().chain(Some(0)).collect();
    let mut blob: Vec<u8> = secret.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(user.as_mut_ptr()),
        ..Default::default()
    };
    let result = unsafe { CredWriteW(&credential, 0) }.map_err(|e| e.message());
    blob.iter_mut().for_each(|b| *b = 0);
    result
}

/// The secret stored for connection `id`.
pub fn load(id: u64) -> Option<String> {
    let name = target(id);
    let mut found: *mut CREDENTIALW = std::ptr::null_mut();
    unsafe {
        CredReadW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None, &mut found).ok()?;
        let credential = &*found;
        let bytes = std::slice::from_raw_parts(
            credential.CredentialBlob,
            credential.CredentialBlobSize as usize,
        );
        let secret = String::from_utf8(bytes.to_vec()).ok();
        CredFree(found as *const _);
        secret
    }
}

pub fn remove(id: u64) {
    let name = target(id);
    unsafe {
        let _ = CredDeleteW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None);
    }
}

// --- SSH host keys (trust on first use) -------------------------------------

fn host_keys_path() -> Option<std::path::PathBuf> {
    crate::core::app_data::data_dir().map(|d| d.join("remote_host_keys.json"))
}

fn host_keys() -> HashMap<String, String> {
    host_keys_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

/// Checks a server's SSH host key fingerprint against the one seen the
/// first time (which is remembered). A different key means the server
/// changed, or someone is impersonating it, so the connection is refused.
pub fn check_host_key(host: &str, port: u16, fingerprint: &str) -> Result<(), String> {
    let key = format!("{host}:{port}");
    let mut keys = host_keys();
    let mut problems = PROBLEMS.lock().unwrap_or_else(|e| e.into_inner());
    problems.remove(&key);
    match keys.get(&key) {
        Some(known) if known == fingerprint => Ok(()),
        Some(known) => Err(problems
            .entry(key)
            .or_insert(format!(
                "The server's host key has changed (it was {known}, it is now {fingerprint}). \
             This can mean someone is intercepting the connection. If the server really was \
             reinstalled, remove and add this network location again."
            ))
            .clone()),
        None => {
            keys.insert(key, fingerprint.to_string());
            if let Some(path) = host_keys_path() {
                let _ = std::fs::write(path, serde_json::to_vec_pretty(&keys).unwrap_or_default());
            }
            Ok(())
        }
    }
}

static PROBLEMS: std::sync::Mutex<std::collections::BTreeMap<String, String>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Why the last connection to `host:port` was refused over its host key.
pub fn host_key_problem(host: &str, port: u16) -> Option<String> {
    PROBLEMS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&format!("{host}:{port}"))
        .cloned()
}

/// Forgets the remembered host key (when a location is removed).
pub fn forget_host_key(host: &str, port: u16) {
    let mut keys = host_keys();
    if keys.remove(&format!("{host}:{port}")).is_some()
        && let Some(path) = host_keys_path()
    {
        let _ = std::fs::write(path, serde_json::to_vec_pretty(&keys).unwrap_or_default());
    }
}
