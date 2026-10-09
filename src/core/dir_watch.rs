//! Watching a folder for changes after a custom context menu command.
//!
//! A custom command runs detached, and often several processes deep: a `.vbs`
//! that returns the moment it has handed off to a `.bat`, a `.bat` that appends
//! to a queue and exits, and a PowerShell script - started by whichever of
//! those instances won a lock - that does the real work some seconds later.
//! There is no process for this app to wait on (the one it started is long
//! gone), and no useful way to guess how long the work will take: writing a
//! text file is instant, zipping a large folder is minutes.
//!
//! So rather than timing it, watch the folder and refresh when something
//! actually lands in it.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Stop once this long has passed with nothing further changing. Long enough to
/// cover a script that thinks before it writes (the queue-draining one this was
/// built for sleeps ~1s deciding whether more items are still arriving).
const QUIET: Duration = Duration::from_secs(20);
/// Never watch longer than this in total, however busy the folder stays.
const MAX_WATCH: Duration = Duration::from_secs(10 * 60);
/// A single operation writes many times; coalesce a burst into one refresh.
const DEBOUNCE: Duration = Duration::from_millis(400);
/// Never ask for refreshes faster than this while work is still going on. A
/// file being written signals continuously (every size and last-write change),
/// and a refresh is a full directory listing - without this, zipping a large
/// folder would re-list its parent hundreds of times while it ran. A final
/// refresh is always issued once the folder goes quiet, so the end state is
/// accurate regardless of what this skipped.
const MIN_REFRESH_GAP: Duration = Duration::from_secs(2);
/// How long to block in each wait before re-checking the deadlines above.
const POLL_MS: u32 = 1000;
/// Invoking a command repeatedly must not pile up threads without limit.
const MAX_WATCHERS: usize = 4;

/// The folder a watcher last saw change, picked up by the UI once per frame.
static CHANGED: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Mirrors "`CHANGED` holds something" so the per-frame check in the UI costs a
/// relaxed atomic load rather than taking a lock on every single frame.
static HAS_CHANGE: AtomicBool = AtomicBool::new(false);
static WATCHERS: AtomicUsize = AtomicUsize::new(0);

/// The folder that changed since the last call, if any.
pub fn take_changed() -> Option<PathBuf> {
    if !HAS_CHANGE.load(Ordering::Relaxed) {
        return None;
    }
    HAS_CHANGE.store(false, Ordering::Relaxed);
    CHANGED.lock().ok().and_then(|mut c| c.take())
}

fn note_change(dir: &Path) {
    if let Ok(mut changed) = CHANGED.lock() {
        *changed = Some(dir.to_path_buf());
    }
    HAS_CHANGE.store(true, Ordering::Relaxed);
    crate::gui::windows::windowsoverrides::request_repaint();
}

/// Watches `dir` until it goes quiet, flagging a refresh whenever its contents
/// change. Cheap to call for a command that turns out to write nothing: the
/// watcher simply times out after `QUIET` having flagged nothing.
pub fn watch(dir: &Path) {
    if !dir.is_dir() {
        return;
    }
    if WATCHERS.load(Ordering::Relaxed) >= MAX_WATCHERS {
        return;
    }
    let dir = dir.to_path_buf();
    WATCHERS.fetch_add(1, Ordering::Relaxed);
    std::thread::spawn(move || {
        run(&dir);
        WATCHERS.fetch_sub(1, Ordering::Relaxed);
    });
}

fn run(dir: &Path) {
    use windows::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::Storage::FileSystem::{
        FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
        FILE_NOTIFY_CHANGE_SIZE, FindCloseChangeNotification, FindFirstChangeNotificationW,
        FindNextChangeNotification,
    };
    use windows::Win32::System::Threading::WaitForSingleObject;
    use windows::core::HSTRING;

    // Names cover a file appearing or being renamed into place; size and last
    // write cover one being filled in afterwards, which is what a zip being
    // built in place looks like.
    let filter = FILE_NOTIFY_CHANGE_FILE_NAME
        | FILE_NOTIFY_CHANGE_DIR_NAME
        | FILE_NOTIFY_CHANGE_SIZE
        | FILE_NOTIFY_CHANGE_LAST_WRITE;

    // `false`: this folder only. A command that writes into a subfolder is not
    // something the view would show anyway.
    let handle = match unsafe { FindFirstChangeNotificationW(&HSTRING::from(dir), false, filter) } {
        Ok(handle) if !handle.is_invalid() => handle,
        _ => return,
    };

    let started = Instant::now();
    let mut last_change = Instant::now();
    // `None` until the first change, so a command that writes nothing at all
    // never triggers a refresh.
    let mut last_refresh: Option<Instant> = None;
    let mut unreported = false;

    loop {
        if started.elapsed() >= MAX_WATCH || last_change.elapsed() >= QUIET {
            break;
        }
        let waited = unsafe { WaitForSingleObject(handle, POLL_MS) };
        if waited == WAIT_TIMEOUT {
            continue;
        }
        if waited != WAIT_OBJECT_0 {
            break;
        }
        last_change = Instant::now();
        // Let the rest of the burst land before considering a refresh.
        std::thread::sleep(DEBOUNCE);
        unreported = true;

        // Refresh while the work is in progress, but no faster than
        // `MIN_REFRESH_GAP` - the folder may be signalling continuously.
        if last_refresh.is_none_or(|at| at.elapsed() >= MIN_REFRESH_GAP) {
            note_change(dir);
            last_refresh = Some(Instant::now());
            unreported = false;
        }

        if unsafe { FindNextChangeNotification(handle) }.is_err() {
            break;
        }
    }

    // The folder has gone quiet (or the cap was hit). If anything changed since
    // the last refresh, this is the one that shows the finished result.
    if unreported {
        note_change(dir);
    }
    unsafe {
        let _ = FindCloseChangeNotification(handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_is_reported_once_and_then_cleared() {
        let dir = std::env::temp_dir().join("eden-dir-watch-test");
        note_change(&dir);
        assert_eq!(take_changed().as_deref(), Some(dir.as_path()));
        assert_eq!(take_changed(), None, "a change must not be reported twice");
    }

    #[test]
    fn watching_something_that_is_not_a_folder_starts_no_thread() {
        let before = WATCHERS.load(Ordering::Relaxed);
        watch(Path::new(r"C:\definitely\not\a\real\folder\anywhere"));
        assert_eq!(WATCHERS.load(Ordering::Relaxed), before);
    }

    #[test]
    fn refreshes_are_rate_limited_well_under_the_quiet_period() {
        // The in-progress refresh gap has to be short enough to show progress
        // but long enough that a continuously-written file cannot turn into a
        // storm of directory listings.
        assert!(MIN_REFRESH_GAP > DEBOUNCE);
        assert!(MIN_REFRESH_GAP < QUIET);
    }

    /// Ignored because it takes a couple of seconds and leans on real
    /// filesystem notifications. Run it on demand with:
    ///
    ///     cargo test -- --ignored a_file_appearing
    ///
    /// It covers the part that cannot be checked by reading the code: that
    /// Windows actually signals the handle, that the debounce and the
    /// rate limit do not swallow the very first change, and that the path
    /// handed back is the folder that was watched.
    #[test]
    #[ignore = "takes a few seconds and uses real filesystem notifications"]
    fn a_file_appearing_in_a_watched_folder_is_reported() {
        let dir = std::env::temp_dir().join("eden-dir-watch-live");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch folder");
        let _ = take_changed(); // clear anything left by another test

        watch(&dir);
        // Let the watcher reach its wait before anything changes.
        std::thread::sleep(Duration::from_millis(300));
        std::fs::write(dir.join("result.zip"), b"pretend archive").expect("write");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen = None;
        while Instant::now() < deadline {
            if let Some(path) = take_changed() {
                seen = Some(path);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            seen.as_deref(),
            Some(dir.as_path()),
            "a file appearing in the watched folder should have been reported"
        );
    }

    #[test]
    fn the_quiet_period_outlasts_the_debounce_it_contains() {
        // If a burst could be coalesced for longer than the watcher is willing
        // to wait, a slow writer would end up never reported.
        assert!(QUIET > DEBOUNCE);
        assert!(MAX_WATCH > QUIET);
    }
}
