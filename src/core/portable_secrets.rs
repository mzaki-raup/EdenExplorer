//! Network-location passwords carried with a **portable** install, encrypted
//! under a passphrase the user chooses.
//!
//! Normally a secret lives in Windows Credential Manager (`remote::
//! credentials`), which is per Windows user and per machine - exactly right
//! for an installed copy, and exactly wrong for a copy on a USB stick, which
//! arrives at the next machine with its connections but none of their
//! passwords.
//!
//! **The passphrase is the whole design, and the alternatives were rejected
//! deliberately:**
//!
//! * *Windows DPAPI* binds its key to the user or the machine, so the
//!   ciphertext cannot be decrypted anywhere else - which is the one thing
//!   this has to do.
//! * *A key embedded in the program* would be in the published source and the
//!   published binary, so anyone holding the stick holds the passwords. The
//!   real harm there is not the weakness but the label: it would describe
//!   itself as encrypted and invite people to treat a lost stick as harmless.
//!
//! So the key comes from the user and nowhere else. Lose the passphrase and
//! the secrets are gone - which is the correct behaviour, and is said plainly
//! in the UI rather than worked around.
//!
//! Argon2id (not PBKDF2) because the input is a human-chosen passphrase and
//! Argon2 is memory-hard; AES-256-GCM because it authenticates, so a wrong
//! passphrase fails cleanly instead of yielding plausible rubbish.
//!
//! **This file is deliberately not part of `SettingsExportBundle`.** Export
//! Settings is something people send to each other; it carries settings,
//! favourites, tags and themes, and must never start carrying passwords.
//! `the_export_bundle_cannot_reach_the_vault` pins that.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use zeroize::Zeroize;

const FILE: &str = "portable_secrets.json";
const FORMAT_VERSION: u32 = 1;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const SALT_LEN: usize = 16;

/// Sealed under the key when a vault is created, and opened to tell a correct
/// passphrase from a wrong one. AES-GCM authenticates, so "it decrypted" is
/// itself the proof - no password hash is stored.
const CHECK_PHRASE: &[u8] = b"EdenExplorer portable secrets v1";

/// OWASP's recommended Argon2id settings (19 MiB, 2 passes, 1 lane). Stored
/// in the file rather than assumed, so raising them later still opens an
/// older vault.
const DEFAULT_M_COST: u32 = 19 * 1024;
const DEFAULT_T_COST: u32 = 2;
const DEFAULT_P_COST: u32 = 1;

#[derive(Clone, Serialize, Deserialize)]
struct Kdf {
    algorithm: String,
    salt: String,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
}

#[derive(Clone, Serialize, Deserialize)]
struct Sealed {
    nonce: String,
    ciphertext: String,
}

/// The on-disk shape. JSON rather than `postcard`: this file gains fields
/// over time and postcard's positional format turns an appended field into a
/// whole-struct decode failure (see `CLAUDE.md`), which here would mean
/// silently losing every stored password.
#[derive(Clone, Serialize, Deserialize)]
pub struct Vault {
    format_version: u32,
    kdf: Kdf,
    check: Sealed,
    /// Connection id (as a string, since JSON object keys are strings) to its
    /// sealed secret.
    secrets: BTreeMap<String, Sealed>,
}

/// A derived key, wiped when it goes out of scope.
pub struct Key([u8; KEY_LEN]);

impl Drop for Key {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

// --- hex ----------------------------------------------------------------

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((byte & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let bytes = text.as_bytes();
    (0..bytes.len() / 2)
        .map(|i| u8::from_str_radix(text.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect()
}

fn random(len: usize) -> Vec<u8> {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..len).map(|_| rng.random::<u8>()).collect()
}

// --- crypto -------------------------------------------------------------

fn derive(passphrase: &str, kdf: &Kdf) -> Result<Key, String> {
    let salt = unhex(&kdf.salt).ok_or("The stored salt is not readable.")?;
    let params = Params::new(kdf.m_cost, kdf.t_cost, kdf.p_cost, Some(KEY_LEN))
        .map_err(|e| format!("Those key settings are not usable ({e})."))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; KEY_LEN];
    argon
        .hash_password_into(passphrase.as_bytes(), &salt, &mut key)
        .map_err(|e| format!("The passphrase could not be processed ({e})."))?;
    Ok(Key(key))
}

fn seal(key: &Key, plaintext: &[u8]) -> Result<Sealed, String> {
    let cipher = Aes256Gcm::new_from_slice(&key.0).map_err(|e| e.to_string())?;
    // A fresh nonce every time. Reusing one under the same key is the single
    // way to break GCM outright, so it is generated here and never derived
    // from anything about the message.
    let nonce_bytes = random(NONCE_LEN);
    let nonce = Nonce::try_from(nonce_bytes.as_slice())
        .map_err(|_| "The nonce was the wrong length.".to_string())?;
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| "The secret could not be encrypted.".to_string())?;
    Ok(Sealed {
        nonce: hex(&nonce_bytes),
        ciphertext: hex(&ciphertext),
    })
}

fn open(key: &Key, sealed: &Sealed) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(&key.0).ok()?;
    let nonce_bytes = unhex(&sealed.nonce)?;
    if nonce_bytes.len() != NONCE_LEN {
        return None;
    }
    let ciphertext = unhex(&sealed.ciphertext)?;
    let nonce = Nonce::try_from(nonce_bytes.as_slice()).ok()?;
    cipher.decrypt(&nonce, ciphertext.as_slice()).ok()
}

impl Vault {
    /// A brand-new vault, plus the key it was created under.
    pub fn create(passphrase: &str) -> Result<(Vault, Key), String> {
        if passphrase.is_empty() {
            return Err("Choose a passphrase first.".into());
        }
        let kdf = Kdf {
            algorithm: "argon2id".into(),
            salt: hex(&random(SALT_LEN)),
            m_cost: DEFAULT_M_COST,
            t_cost: DEFAULT_T_COST,
            p_cost: DEFAULT_P_COST,
        };
        let key = derive(passphrase, &kdf)?;
        let check = seal(&key, CHECK_PHRASE)?;
        Ok((
            Vault {
                format_version: FORMAT_VERSION,
                kdf,
                check,
                secrets: BTreeMap::new(),
            },
            key,
        ))
    }

    /// The key for `passphrase`, or `None` when it is the wrong one. The
    /// check blob is authenticated, so a wrong passphrase cannot open it.
    pub fn unlock(&self, passphrase: &str) -> Option<Key> {
        let key = derive(passphrase, &self.kdf).ok()?;
        (open(&key, &self.check)? == CHECK_PHRASE).then_some(key)
    }

    pub fn put(&mut self, key: &Key, id: u64, secret: &str) -> Result<(), String> {
        self.secrets.insert(id.to_string(), seal(key, secret.as_bytes())?);
        Ok(())
    }

    pub fn get(&self, key: &Key, id: u64) -> Option<String> {
        let mut bytes = open(key, self.secrets.get(&id.to_string())?)?;
        let secret = String::from_utf8(bytes.clone()).ok();
        bytes.zeroize();
        secret
    }

    pub fn forget(&mut self, id: u64) {
        self.secrets.remove(&id.to_string());
    }

    /// The connections this vault holds a secret for.
    pub fn ids(&self) -> Vec<u64> {
        self.secrets.keys().filter_map(|k| k.parse().ok()).collect()
    }
}

// --- the file, and the unlocked key for this session --------------------

/// Where the vault lives: beside the rest of the data, so a portable install
/// carries it on the stick and an installed one keeps it with its settings.
pub fn path() -> Option<PathBuf> {
    crate::core::app_data::data_dir().map(|dir| dir.join(FILE))
}

pub fn exists() -> bool {
    path().is_some_and(|p| p.is_file())
}

fn read() -> Option<Vault> {
    serde_json::from_slice(&std::fs::read(path()?).ok()?).ok()
}

fn write(vault: &Vault) -> Result<(), String> {
    let path = path().ok_or("There is nowhere to save it.")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let data = serde_json::to_vec_pretty(vault).map_err(|e| e.to_string())?;
    std::fs::write(path, data).map_err(|e| e.to_string())
}

/// The key for this session. Held only in memory - closing the app forgets
/// it, which is the point.
static UNLOCKED: Mutex<Option<[u8; KEY_LEN]>> = Mutex::new(None);

fn session_key() -> Option<Key> {
    UNLOCKED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|bytes| Key(*bytes))
}

fn remember(key: &Key) {
    *UNLOCKED.lock().unwrap_or_else(|e| e.into_inner()) = Some(key.0);
}

pub fn is_unlocked() -> bool {
    UNLOCKED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some()
}

/// Forgets the key until the passphrase is entered again.
pub fn lock() {
    if let Some(mut key) = UNLOCKED.lock().unwrap_or_else(|e| e.into_inner()).take() {
        key.zeroize();
    }
}

/// Starts carrying secrets, under `passphrase`.
pub fn enable(passphrase: &str) -> Result<(), String> {
    if exists() {
        return Err("Secrets are already being carried with this copy.".into());
    }
    let (vault, key) = Vault::create(passphrase)?;
    write(&vault)?;
    remember(&key);
    Ok(())
}

/// Stops carrying them, and deletes what was stored. The copies in Windows
/// Credential Manager are untouched.
pub fn disable() -> Result<(), String> {
    lock();
    match path() {
        Some(path) if path.is_file() => std::fs::remove_file(path).map_err(|e| e.to_string()),
        _ => Ok(()),
    }
}

pub fn unlock(passphrase: &str) -> Result<(), String> {
    let vault = read().ok_or("There are no carried secrets to unlock.")?;
    let key = vault
        .unlock(passphrase)
        .ok_or("That passphrase does not open the stored secrets.")?;
    remember(&key);
    Ok(())
}

/// Adds or replaces the secret carried for `id`. Does nothing when the vault
/// is locked or not in use, so ordinary credential saving is unaffected.
pub fn store(id: u64, secret: &str) -> Result<(), String> {
    let (Some(mut vault), Some(key)) = (read(), session_key()) else {
        return Ok(());
    };
    if secret.is_empty() {
        vault.forget(id);
    } else {
        vault.put(&key, id, secret)?;
    }
    write(&vault)
}

/// The carried secret for `id`, when there is one and the vault is open.
pub fn load(id: u64) -> Option<String> {
    read()?.get(&session_key()?, id)
}

pub fn remove(id: u64) {
    if let Some(mut vault) = read() {
        vault.forget(id);
        let _ = write(&vault);
    }
}

/// Which connections have a carried secret - readable without unlocking,
/// since the ids are not themselves secret.
pub fn carried_ids() -> Vec<u64> {
    read().map(|v| v.ids()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cheap settings so the tests are not dominated by key derivation. The
    /// real ones are in `DEFAULT_*`; `the_real_parameters_are_used_for_a_new_vault`
    /// pins those.
    fn fast(passphrase: &str) -> (Vault, Key) {
        let kdf = Kdf {
            algorithm: "argon2id".into(),
            salt: hex(&random(SALT_LEN)),
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        };
        let key = derive(passphrase, &kdf).expect("derive");
        let check = seal(&key, CHECK_PHRASE).expect("seal");
        (
            Vault {
                format_version: FORMAT_VERSION,
                kdf,
                check,
                secrets: BTreeMap::new(),
            },
            key,
        )
    }

    #[test]
    fn hex_round_trips_including_the_awkward_bytes() {
        for bytes in [vec![], vec![0x00], vec![0xff, 0x00, 0x0f, 0xf0], random(64)] {
            assert_eq!(unhex(&hex(&bytes)).as_deref(), Some(bytes.as_slice()));
        }
        assert_eq!(unhex("abc"), None, "odd length is not hex");
        assert_eq!(unhex("zz"), None, "non-hex digits are refused");
    }

    #[test]
    fn a_secret_round_trips_through_the_vault() {
        let (mut vault, key) = fast("correct horse battery staple");
        vault.put(&key, 900_001, "hunter2").expect("put");
        assert_eq!(vault.get(&key, 900_001).as_deref(), Some("hunter2"));
        assert_eq!(vault.get(&key, 900_002), None, "nothing stored for that id");
    }

    /// The point of the whole feature: the file travels, so it must open on a
    /// machine that has only the passphrase.
    #[test]
    fn a_vault_serialised_and_reloaded_opens_with_the_passphrase() {
        let (mut vault, key) = fast("a passphrase");
        vault.put(&key, 7, "sftp-password").expect("put");
        let json = serde_json::to_vec(&vault).expect("serialise");

        let reloaded: Vault = serde_json::from_slice(&json).expect("deserialise");
        let reopened = reloaded.unlock("a passphrase").expect("unlock");
        assert_eq!(reloaded.get(&reopened, 7).as_deref(), Some("sftp-password"));
    }

    #[test]
    fn the_wrong_passphrase_opens_nothing() {
        let (mut vault, key) = fast("right");
        vault.put(&key, 1, "secret").expect("put");
        assert!(vault.unlock("wrong").is_none());
        assert!(vault.unlock("").is_none());
        assert!(vault.unlock("Right").is_none(), "case matters");
    }

    /// A wrong key must fail, not return rubbish - which is why the cipher is
    /// authenticated rather than a bare stream.
    #[test]
    fn a_wrong_key_cannot_decrypt_to_anything_at_all() {
        let (mut vault, key) = fast("right");
        vault.put(&key, 1, "secret").expect("put");
        let (_, other) = fast("different");
        assert_eq!(vault.get(&other, 1), None);
    }

    /// Tampering with the stored bytes must be detected rather than produce a
    /// corrupted password that gets sent to a server.
    #[test]
    fn altered_ciphertext_is_refused() {
        let (mut vault, key) = fast("p");
        vault.put(&key, 1, "secret").expect("put");
        let sealed = vault.secrets.get_mut("1").expect("stored");
        let mut bytes = unhex(&sealed.ciphertext).expect("hex");
        bytes[0] ^= 0x01;
        sealed.ciphertext = hex(&bytes);
        assert_eq!(vault.get(&key, 1), None);
    }

    /// Reusing a nonce under one key breaks GCM outright, so two secrets -
    /// even identical ones - must never share one.
    #[test]
    fn every_secret_gets_its_own_nonce() {
        let (mut vault, key) = fast("p");
        for id in 1..=20u64 {
            vault.put(&key, id, "the same secret every time").expect("put");
        }
        let mut nonces: Vec<&str> = vault.secrets.values().map(|s| s.nonce.as_str()).collect();
        nonces.sort_unstable();
        let before = nonces.len();
        nonces.dedup();
        assert_eq!(nonces.len(), before, "a nonce was reused");

        // Identical plaintexts must also not produce identical ciphertexts.
        let mut texts: Vec<&str> = vault.secrets.values().map(|s| s.ciphertext.as_str()).collect();
        texts.sort_unstable();
        let before = texts.len();
        texts.dedup();
        assert_eq!(texts.len(), before, "identical secrets encrypted alike");
    }

    #[test]
    fn replacing_and_forgetting_a_secret() {
        let (mut vault, key) = fast("p");
        vault.put(&key, 1, "first").expect("put");
        vault.put(&key, 1, "second").expect("replace");
        assert_eq!(vault.get(&key, 1).as_deref(), Some("second"));
        assert_eq!(vault.ids(), vec![1]);
        vault.forget(1);
        assert_eq!(vault.get(&key, 1), None);
        assert!(vault.ids().is_empty());
    }

    /// The stored file must not contain the secret in any recoverable form.
    #[test]
    fn the_plaintext_never_appears_in_the_file() {
        // Both distinctive: a short passphrase like "p" occurs inside
        // "p_cost" and "ciphertext" and would fail for no real reason.
        let (mut vault, key) = fast("zzphrasezz");
        vault.put(&key, 1, "super-secret-password").expect("put");
        let json =
            String::from_utf8(serde_json::to_vec_pretty(&vault).expect("json")).expect("utf8");
        assert!(!json.contains("super-secret-password"), "the secret is in the file");
        assert!(!json.contains("zzphrasezz"), "the passphrase is in the file");
    }

    #[test]
    fn an_empty_passphrase_cannot_create_a_vault() {
        assert!(Vault::create("").is_err());
    }

    #[test]
    fn the_real_parameters_are_used_for_a_new_vault() {
        let (vault, _key) = Vault::create("a real one").expect("create");
        assert_eq!(vault.kdf.algorithm, "argon2id");
        assert_eq!(vault.kdf.m_cost, 19 * 1024, "OWASP's 19 MiB");
        assert_eq!(vault.kdf.t_cost, 2);
        assert_eq!(vault.format_version, FORMAT_VERSION);
        assert_eq!(
            unhex(&vault.kdf.salt).map(|s| s.len()),
            Some(SALT_LEN),
            "a full-length random salt"
        );
    }

    /// Two vaults made with the same passphrase must still differ, or the
    /// salt is not doing its job.
    #[test]
    fn two_vaults_with_one_passphrase_use_different_salts() {
        let (a, _) = Vault::create("same").expect("a");
        let (b, _) = Vault::create("same").expect("b");
        assert_ne!(a.kdf.salt, b.kdf.salt);
        assert_ne!(a.check.ciphertext, b.check.ciphertext);
    }

    /// Export Settings is something people send to each other. It carries
    /// settings, favourites, tags and themes - and must never grow a field
    /// that reaches this vault.
    #[test]
    fn the_export_bundle_cannot_reach_the_vault() {
        let bundle = crate::core::indexer::SettingsExportBundle {
            format_version: 1,
            settings: Default::default(),
            favorites: Vec::new(),
            tags: None,
            theme_light: crate::gui::theme::get_default_palette(
                crate::gui::theme::ThemeMode::Light,
            ),
            theme_dark: crate::gui::theme::get_default_palette(
                crate::gui::theme::ThemeMode::Dark,
            ),
        };
        let json = serde_json::to_string(&bundle).expect("serialise");
        for forbidden in ["portable_secrets", "passphrase", "ciphertext", "argon2id"] {
            assert!(
                !json.contains(forbidden),
                "the export bundle mentions {forbidden}"
            );
        }
    }
}
