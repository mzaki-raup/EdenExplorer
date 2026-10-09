//! Fetching, verifying and installing a newer release.
//!
//! `core::release` decides *whether* a published release is worth offering;
//! this module does the parts that touch the network and the filesystem. All of
//! it runs on helper threads and reports back through `drain_events`, which the
//! UI drains once per frame - nothing here blocks a frame.

use crate::core::indexer;
use crate::core::release::{self, Update, Version};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Nothing the releases API answers with should be anywhere near this large.
const MAX_JSON: u64 = 1 << 20;
/// Nor should the program be - it is tens of megabytes, not hundreds.
const MAX_EXE: u64 = 256 << 20;
/// What the running program is renamed to while it is replaced. Windows refuses
/// to delete a running image but will happily rename it, which is the whole
/// trick behind `install` below.
const OLD_EXE: &str = "EdenExplorer.old.exe";
/// How often the automatic check runs.
const CHECK_EVERY: u64 = 24 * 60 * 60;
/// How long to keep trying to delete the replaced image at startup. `restart`
/// spawns the new copy and then exits, so for a moment both are alive and
/// Windows still holds the old image open - a single attempt loses that race
/// and leaves tens of megabytes behind until some later start happens to win it.
const OLD_EXE_ATTEMPTS: u32 = 20;
const OLD_EXE_RETRY: std::time::Duration = std::time::Duration::from_millis(500);

/// Where a download is staged before it is installed.
fn staging_dir() -> PathBuf {
    std::env::temp_dir().join("EdenExplorer Update")
}

/// What a background check or download finished doing.
#[derive(Debug)]
pub enum Event {
    /// A check the user asked for that found nothing newer. An automatic check
    /// never reports this - it only speaks up when there is something to say.
    UpToDate,
    Available(Update),
    /// A newer release exists, but it has no program attached - so there is
    /// nothing to download and nothing to verify it with. Reported instead of
    /// `UpToDate`, which is what this used to look like.
    Incomplete { version: Version, page: String },
    /// A check or a download failed. The string is already user-facing.
    Failed(String),
    /// Verified and ready to install.
    Downloaded { update: Update, file: PathBuf },
}

static INBOX: Mutex<Vec<Event>> = Mutex::new(Vec::new());
/// One check or download at a time: the UI can offer "check now" while an
/// automatic check is still in flight.
static BUSY: Mutex<bool> = Mutex::new(false);

fn post(event: Event) {
    if let Ok(mut inbox) = INBOX.lock() {
        inbox.push(event);
    }
    crate::gui::windows::windowsoverrides::request_repaint();
}

/// Everything that finished since the last call. Drained once per frame.
pub fn drain_events() -> Vec<Event> {
    INBOX
        .lock()
        .map(|mut inbox| std::mem::take(&mut *inbox))
        .unwrap_or_default()
}

fn claim() -> bool {
    match BUSY.lock() {
        Ok(mut busy) if !*busy => {
            *busy = true;
            true
        }
        _ => false,
    }
}

fn unclaim() {
    if let Ok(mut busy) = BUSY.lock() {
        *busy = false;
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// An HTTP client using Windows' own TLS, matching `core::remote`'s agents.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                // ureq defaults to `RootCerts::WebPki` (Mozilla's bundled
                // roots), which native-tls then treats as *user-specified*
                // roots - and fails the handshake outright with "unable to
                // find any user-specified roots in the final cert chain".
                // `PlatformVerifier` is what makes native-tls use the roots
                // it loads by default, i.e. the Windows certificate store.
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .http_status_as_error(false)
        .timeout_connect(Some(std::time::Duration::from_secs(15)))
        .build()
        .into()
}

/// One GET into memory, with a cap. Read through a `take` rather than any
/// client-side limit setting so the cap does not depend on an ureq version's
/// own configuration API.
fn get(agent: &ureq::Agent, url: &str, max: u64) -> Result<Vec<u8>, String> {
    let response = agent
        .get(url)
        .call()
        .map_err(|e| format!("{e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    let mut body = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(max)
        .read_to_end(&mut body)
        .map_err(|e| format!("{e}"))?;
    if body.len() as u64 >= max {
        return Err("the download is larger than expected".to_string());
    }
    Ok(body)
}

/// Clears what a previous update left behind: the image it replaced, and any
/// download that was staged but never installed. Both are the size of the whole
/// program, so leaving them is not harmless.
///
/// Runs on a helper thread: deleting the old image has to be retried (see
/// `OLD_EXE_ATTEMPTS`) and the first frame should not wait for that.
pub fn clean_leftovers() {
    std::thread::spawn(|| {
        // First, and without retrying: a download that was fetched but never
        // installed is nobody's lock. Doing it before the retry loop also keeps
        // this well clear of any download the user starts moments later.
        let _ = std::fs::remove_dir_all(staging_dir());

        let Ok(current) = std::env::current_exe() else {
            return;
        };
        let old = current.with_file_name(OLD_EXE);
        for _ in 0..OLD_EXE_ATTEMPTS {
            if !old.exists() || std::fs::remove_file(&old).is_ok() {
                return;
            }
            // Still held by the copy that is on its way out; give it a moment.
            std::thread::sleep(OLD_EXE_RETRY);
        }
        // Still there: the next start tries again, which is why this is not an
        // error anyone needs to see.
    });
}

/// Whether this is a release the user chose to skip. Anything newer than the
/// skipped one is still offered.
fn skipped(update: &Update) -> bool {
    let state = indexer::load_update_state();
    Version::parse(&state.skipped).is_some_and(|skip| update.version <= skip)
}

/// Remembers that this release was skipped, so it is not offered again by
/// itself.
pub fn skip(update: &Update) {
    let mut state = indexer::load_update_state();
    state.skipped = update.version.to_string();
    indexer::save_update_state(&state);
}

/// Looks for a newer release. `manual` is the user having asked, which reports
/// whatever it finds; an automatic check runs at most daily, respects the
/// "check automatically" setting, and stays quiet unless there is an update.
pub fn check(manual: bool) {
    if !manual {
        let state = indexer::load_update_state();
        if !state.auto_check || now_secs().saturating_sub(state.last_check) < CHECK_EVERY {
            return;
        }
    }
    if !claim() {
        return;
    }
    std::thread::spawn(move || {
        let found = fetch(release::RELEASES_API);
        unclaim();

        // Recorded even when the check failed, so an unreachable forge is not
        // retried on every single launch.
        let mut state = indexer::load_update_state();
        state.last_check = now_secs();
        indexer::save_update_state(&state);

        match found {
            Err(error) => {
                if manual {
                    post(Event::Failed(error));
                }
            }
            Ok(Found::None { newest }) => {
                // A newer release that could not be offered is worth saying
                // out loud: it means the publish did not finish, and claiming
                // the user is up to date hides that from everyone at once.
                match newest {
                    Some(newest) if newest > Version::running() => {
                        post(Event::Incomplete {
                            version: newest,
                            page: releases_page(),
                        });
                    }
                    _ => {
                        if manual {
                            post(Event::UpToDate);
                        }
                    }
                }
            }
            Ok(Found::Update(update)) => {
                let update = *update;
                // A skipped release is still reported for a check the user
                // asked for - they went looking.
                if manual || !skipped(&update) {
                    post(Event::Available(update));
                }
            }
        }
    });
}

/// What a check found: an update, or - when there is none to offer - the
/// newest version published, so an incomplete release can be named rather
/// than silently ignored.
enum Found {
    Update(Box<Update>),
    None { newest: Option<Version> },
}

fn fetch(url: &str) -> Result<Found, String> {
    let body = get(&agent(), url, MAX_JSON)?;
    let json = String::from_utf8(body).map_err(|_| "the forge's answer was not text".to_string())?;
    Ok(match release::newer_than(&json, Version::running()) {
        Some(update) => Found::Update(Box::new(update)),
        None => Found::None {
            newest: release::newest_version(&json),
        },
    })
}

/// The page a release with no assets can still be opened at, so "go and look"
/// is actionable.
fn releases_page() -> String {
    release::RELEASES_API
        .split("/api/v1/repos/")
        .nth(1)
        .and_then(|rest| rest.split("/releases").next())
        .map(|repo| format!("https://git.zacx.net/{repo}/releases"))
        .unwrap_or_else(|| "https://git.zacx.net".to_string())
}

/// Downloads the program, checks it against the published checksum, and leaves
/// it in a temporary directory for `install`.
pub fn start_download(update: Update) {
    if !claim() {
        return;
    }
    std::thread::spawn(move || {
        let result = download(&update);
        unclaim();
        match result {
            Ok(file) => post(Event::Downloaded { update, file }),
            Err(error) => post(Event::Failed(error)),
        }
    });
}

fn download(update: &Update) -> Result<PathBuf, String> {
    // No checksums, no automatic install: there would be nothing to verify the
    // download against, and the release workflow always publishes them
    // together, so their absence means something is wrong with the release.
    let sums_asset = update
        .sums
        .as_ref()
        .ok_or_else(|| format!("release {} publishes no {}", update.tag, release::SUMS_ASSET))?;

    let agent = agent();
    let sums_bytes = get(&agent, &sums_asset.url, MAX_JSON)?;
    let sums = String::from_utf8(sums_bytes)
        .map_err(|_| format!("{} was not text", release::SUMS_ASSET))?;
    let expected = release::sha256_of(&sums, release::EXE_ASSET).ok_or_else(|| {
        format!(
            "{} does not list a checksum for {}",
            release::SUMS_ASSET,
            release::EXE_ASSET
        )
    })?;

    let bytes = get(&agent, &update.exe.url, MAX_EXE)?;
    let actual = hex(&Sha256::digest(&bytes));
    if actual != expected {
        return Err(format!(
            "the download does not match its published checksum (expected {}, got {})",
            &expected[..16],
            &actual[..16]
        ));
    }

    let dir = staging_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
    let file = dir.join(release::EXE_ASSET);
    std::fs::write(&file, &bytes).map_err(|e| format!("{e}"))?;
    Ok(file)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Puts `new` where the running program is. The running image is renamed aside
/// first (Windows allows that but not deleting it) and put back if the copy
/// fails, so a failed install never leaves the program missing.
pub fn install(new: &Path) -> Result<(), String> {
    let current = std::env::current_exe().map_err(|e| format!("{e}"))?;
    let old = current.with_file_name(OLD_EXE);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&current, &old).map_err(|e| format!("{e}"))?;
    // A copy, not a rename: the download sits in the temp directory, which is
    // very often on a different volume.
    if let Err(error) = std::fs::copy(new, &current) {
        let _ = std::fs::rename(&old, &current);
        return Err(format!("{error}"));
    }
    // The staged copy is now in place and is the size of the whole program.
    // Only `remove_dir` so this never deletes anything it did not expect - it
    // succeeds exactly when the directory holds nothing but that one file.
    let _ = std::fs::remove_file(new);
    let _ = std::fs::remove_dir(staging_dir());
    Ok(())
}

/// Starts the newly installed program and leaves. Settings and session state
/// are written as they change (and per frame for window geometry), so there is
/// nothing buffered that this would drop.
pub fn restart() {
    if let Ok(exe) = std::env::current_exe() {
        // The copy starts before this process has exited, so it is told it
        // is replacing us: without that it finds our single-instance mutex
        // still held, treats us as "the running instance", and hands its
        // arguments to a program that is about to exit - which ended with
        // no window at all after an update (see `acquire_replacing_previous`).
        let _ = std::process::Command::new(exe)
            .arg(crate::core::launch::REPLACING_PREVIOUS)
            .spawn();
    }
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_is_lower_case_and_zero_padded() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff, 0xa5]), "000fffa5");
    }

    #[test]
    fn a_known_sha256_round_trips_through_hex() {
        // The SHA-256 of the empty input, as `sha256sum` reports it - the same
        // 64-hex-character shape `release::sha256_of` insists on.
        assert_eq!(
            hex(&Sha256::digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn download_and_cleanup_agree_on_where_a_download_is_staged() {
        // `download` writes here and both `install` and `clean_leftovers` remove
        // it; if they ever stopped agreeing, the symptom would be a silent
        // program-sized file left in the temp directory after every update.
        let dir = staging_dir();
        assert!(dir.starts_with(std::env::temp_dir()));
        assert_eq!(
            dir.file_name().and_then(|n| n.to_str()),
            Some("EdenExplorer Update")
        );
    }

    #[test]
    fn the_old_exe_sits_beside_the_running_one() {
        let current = Path::new(r"C:\Program Files\Eden\EdenExplorer.exe");
        assert_eq!(
            current.with_file_name(OLD_EXE),
            Path::new(r"C:\Program Files\Eden\EdenExplorer.old.exe")
        );
    }
}
