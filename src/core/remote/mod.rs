//! Remote locations: SFTP, FTP/FTPS, WebDAV and S3 servers browsed like
//! folders. A remote folder has a path like
//! `\\?\EdenRemote\<connection id>\dir\sub` (a form every Windows file API
//! rejects at once, so nothing that isn't remote-aware ever waits on the
//! network). Each connection gets one worker thread holding its session and
//! running jobs in order; passwords and keys' passphrases live in Windows
//! Credential Manager (`credentials`), never in the settings file.

pub mod credentials;
pub mod ftp;
pub mod s3;
pub mod sftp;
pub mod webdav;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

/// Every remote path starts with this.
pub const PREFIX: &str = r"\\?\EdenRemote\";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteKind {
    Sftp,
    Ftp,
    /// FTP over TLS (explicit, `AUTH TLS`).
    Ftps,
    WebDav,
    S3,
}

impl RemoteKind {
    pub const ALL: [RemoteKind; 5] = [
        RemoteKind::Sftp,
        RemoteKind::Ftp,
        RemoteKind::Ftps,
        RemoteKind::WebDav,
        RemoteKind::S3,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RemoteKind::Sftp => "SFTP",
            RemoteKind::Ftp => "FTP",
            RemoteKind::Ftps => "FTPS",
            RemoteKind::WebDav => "WebDAV",
            RemoteKind::S3 => "S3",
        }
    }

    pub fn default_port(self, https: bool) -> u16 {
        match self {
            RemoteKind::Sftp => 22,
            RemoteKind::Ftp | RemoteKind::Ftps => 21,
            RemoteKind::WebDav | RemoteKind::S3 => {
                if https {
                    443
                } else {
                    80
                }
            }
        }
    }
}

/// A saved remote location (secrets aren't here: see `credentials`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteConnection {
    pub id: u64,
    pub name: String,
    pub kind: RemoteKind,
    /// Server name or address (S3: the endpoint; empty for Amazon S3).
    pub host: String,
    pub port: u16,
    /// User name (S3: the access key ID).
    pub username: String,
    /// SFTP/FTP: the folder to open first. WebDAV: the server path the
    /// location starts at (e.g. `/remote.php/dav/files/me`). S3: unused.
    pub path: String,
    /// WebDAV and S3: use HTTPS.
    pub https: bool,
    /// SFTP: a private key file to sign in with instead of a password (its
    /// passphrase, if any, is the stored secret).
    pub key_file: Option<PathBuf>,
    /// S3 only.
    pub bucket: String,
    pub region: String,
}

impl Default for RemoteConnection {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            kind: RemoteKind::Sftp,
            host: String::new(),
            port: 22,
            username: String::new(),
            path: String::new(),
            https: true,
            key_file: None,
            bucket: String::new(),
            region: "us-east-1".into(),
        }
    }
}

impl RemoteConnection {
    /// How it's shown when it has no name.
    pub fn display_name(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.trim().to_string();
        }
        match self.kind {
            RemoteKind::S3 => format!("S3: {}", self.bucket),
            _ if self.username.is_empty() => format!("{}: {}", self.kind.label(), self.host),
            _ => format!("{}: {}@{}", self.kind.label(), self.username, self.host),
        }
    }

    /// The folder (as path segments) the location opens at.
    pub fn start_segments(&self) -> Vec<String> {
        match self.kind {
            RemoteKind::Sftp | RemoteKind::Ftp | RemoteKind::Ftps => segments_of(&self.path),
            // WebDAV's path is the root itself; S3 starts at the bucket.
            RemoteKind::WebDav | RemoteKind::S3 => Vec::new(),
        }
    }
}

/// One item in a remote folder.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    /// Seconds since 1970 (UTC).
    pub modified: Option<i64>,
}

/// What every protocol can do. Paths are absolute within the location,
/// with `/` separators (`/` is its root).
pub trait RemoteFs: Send {
    fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>, String>;
    fn download(
        &mut self,
        file: &str,
        to: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), String>;
    fn upload(
        &mut self,
        from: &mut dyn Read,
        size: u64,
        file: &str,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), String>;
    fn mkdir(&mut self, dir: &str) -> Result<(), String>;
    fn rename(&mut self, from: &str, to: &str) -> Result<(), String>;
    fn remove_file(&mut self, file: &str) -> Result<(), String>;
    /// Removes an empty folder.
    fn remove_dir(&mut self, dir: &str) -> Result<(), String>;
}

/// Signs in to `conn` (with the stored secret).
pub fn connect(conn: &RemoteConnection) -> Result<Box<dyn RemoteFs>, String> {
    connect_with_secret(conn, &credentials::load(conn.id).unwrap_or_default())
}

/// Signs in to `conn` with `secret` (e.g. one typed but not saved yet).
pub fn connect_with_secret(
    conn: &RemoteConnection,
    secret: &str,
) -> Result<Box<dyn RemoteFs>, String> {
    let secret = secret.to_string();
    match conn.kind {
        RemoteKind::Sftp => Ok(Box::new(sftp::SftpFs::connect(conn, &secret)?)),
        RemoteKind::Ftp | RemoteKind::Ftps => Ok(Box::new(ftp::FtpFs::connect(conn, &secret)?)),
        RemoteKind::WebDav => Ok(Box::new(webdav::WebDavFs::new(conn, &secret)?)),
        RemoteKind::S3 => Ok(Box::new(s3::S3Fs::new(conn, &secret)?)),
    }
}

// --- Paths --------------------------------------------------------------

/// `"/a/b/"` → `["a", "b"]`.
pub fn segments_of(path: &str) -> Vec<String> {
    path.split(['/', '\\'])
        .filter(|s| !s.is_empty() && *s != ".")
        .map(str::to_string)
        .collect()
}

/// The local-looking path of a remote folder or file.
pub fn path_for(id: u64, segments: &[String]) -> PathBuf {
    let mut s = format!("{PREFIX}{id}");
    for segment in segments {
        s.push('\\');
        s.push_str(segment);
    }
    PathBuf::from(s)
}

/// If `path` is remote: its connection id and path segments.
pub fn split(path: &Path) -> Option<(u64, Vec<String>)> {
    let s = path.to_string_lossy();
    let rest = s.strip_prefix(PREFIX)?;
    let mut parts = rest.split('\\').filter(|p| !p.is_empty());
    let id = parts.next()?.parse().ok()?;
    Some((id, parts.map(str::to_string).collect()))
}

pub fn is_remote(path: &Path) -> bool {
    path.to_string_lossy().starts_with(PREFIX)
}

/// `["a", "b"]` → `"/a/b"`.
pub fn remote_path(segments: &[String]) -> String {
    format!("/{}", segments.join("/"))
}

/// A URL for a remote path (`sftp://me@host:22/docs/a.txt`), for Copy Path.
pub fn url_for(path: &Path) -> Option<String> {
    let (id, segments) = split(path)?;
    let conn = connection(id)?;
    let encoded = webdav::encode_path(&remote_path(&segments));
    let user = if conn.username.is_empty() || conn.kind == RemoteKind::S3 {
        String::new()
    } else {
        format!("{}@", conn.username)
    };
    Some(match conn.kind {
        RemoteKind::Sftp => format!("sftp://{user}{}:{}{encoded}", conn.host, conn.port),
        RemoteKind::Ftp => format!("ftp://{user}{}:{}{encoded}", conn.host, conn.port),
        RemoteKind::Ftps => format!("ftps://{user}{}:{}{encoded}", conn.host, conn.port),
        RemoteKind::WebDav => {
            let base = webdav::encode_path(&format!("/{}", segments_of(&conn.path).join("/")));
            let scheme = if conn.https { "https" } else { "http" };
            format!(
                "{scheme}://{}:{}{}{encoded}",
                conn.host,
                conn.port,
                base.trim_end_matches('/')
            )
        }
        RemoteKind::S3 => format!("s3://{}{encoded}", conn.bucket),
    })
}

/// Joins a remote folder and a name.
pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

// --- Saved connections ----------------------------------------------------

static CONNECTIONS: Mutex<Vec<RemoteConnection>> = Mutex::new(Vec::new());

/// Called whenever the saved connections change (and at startup), so
/// background code (previews, thumbnails) can find a path's connection.
pub fn set_connections(connections: &[RemoteConnection]) {
    *CONNECTIONS.lock().unwrap_or_else(|e| e.into_inner()) = connections.to_vec();
}

pub fn connection(id: u64) -> Option<RemoteConnection> {
    CONNECTIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|c| c.id == id)
        .cloned()
}

/// The connection a remote path belongs to.
pub fn connection_of(path: &Path) -> Option<RemoteConnection> {
    split(path).and_then(|(id, _)| connection(id))
}

// --- Workers ----------------------------------------------------------------

type Job = Box<dyn FnOnce(&mut dyn RemoteFs) + Send>;
type JobMsg = (RemoteConnection, Job, Box<dyn FnOnce(String) + Send>);

/// Sessions idle longer than this are re-opened before the next job (servers
/// often drop idle connections).
const IDLE_RECONNECT: Duration = Duration::from_secs(45);

static WORKERS: Mutex<Option<HashMap<u64, Sender<JobMsg>>>> = Mutex::new(None);

fn worker(rx: Receiver<JobMsg>) {
    let mut session: Option<(RemoteConnection, Box<dyn RemoteFs>, Instant)> = None;
    for (conn, job, on_error) in rx {
        let stale = session
            .as_ref()
            .is_some_and(|(c, _, used)| *c != conn || used.elapsed() > IDLE_RECONNECT);
        if stale {
            session = None;
        }
        if session.is_none() {
            match connect(&conn) {
                Ok(fs) => session = Some((conn.clone(), fs, Instant::now())),
                Err(e) => {
                    on_error(e);
                    continue;
                }
            }
        }
        if let Some((_, fs, used)) = session.as_mut() {
            // A job reports its own result; a panic in a backend must not
            // take the worker (and every later job) down with it.
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(fs.as_mut())));
            *used = Instant::now();
            if outcome.is_err() {
                session = None;
            }
        }
    }
}

/// Runs `job` on `conn`'s worker (connecting first if needed). If signing
/// in fails, `on_error` gets the reason instead.
pub fn submit(
    conn: &RemoteConnection,
    job: impl FnOnce(&mut dyn RemoteFs) + Send + 'static,
    on_error: impl FnOnce(String) + Send + 'static,
) {
    let mut workers = WORKERS.lock().unwrap_or_else(|e| e.into_inner());
    let workers = workers.get_or_insert_with(HashMap::new);
    let msg: JobMsg = (conn.clone(), Box::new(job), Box::new(on_error));
    let msg = match workers.get(&conn.id) {
        Some(tx) => match tx.send(msg) {
            Ok(()) => return,
            Err(e) => e.0,
        },
        None => msg,
    };
    let (tx, rx) = channel();
    std::thread::spawn(move || worker(rx));
    let _ = tx.send(msg);
    workers.insert(conn.id, tx);
}

/// Runs `f` on `conn`'s worker and waits for its result.
pub fn run_blocking<T: Send + 'static>(
    conn: &RemoteConnection,
    f: impl FnOnce(&mut dyn RemoteFs) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = channel();
    let tx2 = tx.clone();
    submit(
        conn,
        move |fs| drop(tx.send(f(fs))),
        move |e| drop(tx2.send(Err(e))),
    );
    rx.recv()
        .unwrap_or_else(|_| Err("The connection closed unexpectedly.".into()))
}

/// Closes `id`'s session (after any queued jobs).
pub fn disconnect(id: u64) {
    if let Some(workers) = WORKERS.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        workers.remove(&id);
    }
}

// --- Recursive helpers ------------------------------------------------------

/// Deletes a file, or a folder with everything in it.
pub fn remove_all(fs: &mut dyn RemoteFs, path: &str, is_dir: bool) -> Result<(), String> {
    if !is_dir {
        return fs.remove_file(path);
    }
    for entry in fs.list(path)? {
        remove_all(fs, &join(path, &entry.name), entry.is_dir)?;
    }
    fs.remove_dir(path)
}

/// Downloads a file or a whole folder into `local` (the new item's path).
/// `progress` gets bytes done so far; `cancel` stops between files.
pub fn download_all(
    fs: &mut dyn RemoteFs,
    path: &str,
    is_dir: bool,
    local: &Path,
    progress: &mut dyn FnMut(u64),
    cancel: &dyn Fn() -> bool,
) -> Result<(), String> {
    if cancel() {
        return Err("Cancelled".into());
    }
    if is_dir {
        std::fs::create_dir_all(local).map_err(|e| e.to_string())?;
        for entry in fs.list(path)? {
            let name = safe_local_name(&entry.name)
                .ok_or_else(|| format!("Unsafe name: {}", entry.name))?;
            download_all(
                fs,
                &join(path, &entry.name),
                entry.is_dir,
                &local.join(name),
                progress,
                cancel,
            )?;
        }
        return Ok(());
    }
    let partial = local.with_extension(format!(
        "{}eden-part",
        local
            .extension()
            .map(|e| format!("{}.", e.to_string_lossy()))
            .unwrap_or_default()
    ));
    let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
    let result = fs.download(path, &mut file, progress);
    drop(file);
    match result {
        Ok(()) => std::fs::rename(&partial, local).map_err(|e| e.to_string()),
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e)
        }
    }
}

/// Uploads a local file or folder to `remote` (the new item's path).
pub fn upload_all(
    fs: &mut dyn RemoteFs,
    local: &Path,
    remote: &str,
    progress: &mut dyn FnMut(u64),
    cancel: &dyn Fn() -> bool,
) -> Result<(), String> {
    if cancel() {
        return Err("Cancelled".into());
    }
    let meta = std::fs::metadata(local).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        // The folder may already exist (uploading into it again).
        let _ = fs.mkdir(remote);
        let mut entries: Vec<_> = std::fs::read_dir(local)
            .map_err(|e| e.to_string())?
            .flatten()
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_string();
            upload_all(fs, &entry.path(), &join(remote, &name), progress, cancel)?;
        }
        return Ok(());
    }
    let mut file = std::fs::File::open(local).map_err(|e| e.to_string())?;
    fs.upload(&mut file, meta.len(), remote, progress)
}

/// Total bytes in a local file or folder (for upload progress).
pub fn local_size(path: &Path) -> u64 {
    match std::fs::metadata(path) {
        Ok(m) if m.is_dir() => std::fs::read_dir(path)
            .map(|entries| entries.flatten().map(|e| local_size(&e.path())).sum())
            .unwrap_or(0),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

/// Total bytes in a remote file or folder (for download progress).
pub fn remote_size(
    fs: &mut dyn RemoteFs,
    path: &str,
    entry_size: Option<u64>,
    is_dir: bool,
) -> u64 {
    if !is_dir {
        return entry_size.unwrap_or(0);
    }
    fs.list(path)
        .map(|entries| {
            entries
                .iter()
                .map(|e| remote_size(fs, &join(path, &e.name), e.size, e.is_dir))
                .sum()
        })
        .unwrap_or(0)
}

/// A server-supplied name, if it's safe to use as a local file name (no
/// path separators or `..` that would escape the target folder).
pub fn safe_local_name(name: &str) -> Option<&str> {
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0']);
    (!bad).then_some(name)
}

/// Where opened and previewed remote files are downloaded to.
pub fn temp_root() -> PathBuf {
    std::env::temp_dir().join("EdenExplorer-remote")
}

static SIZES: Mutex<Option<HashMap<PathBuf, u64>>> = Mutex::new(None);

/// Remembers listed files' sizes (so previews know what's too big to fetch).
pub fn remember_sizes(items: impl IntoIterator<Item = (PathBuf, u64)>) {
    let mut sizes = SIZES.lock().unwrap_or_else(|e| e.into_inner());
    let sizes = sizes.get_or_insert_with(HashMap::new);
    if sizes.len() > 200_000 {
        sizes.clear();
    }
    sizes.extend(items);
}

fn known_size(path: &Path) -> Option<u64> {
    SIZES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|s| s.get(path).copied())
}

/// Largest remote file fetched just to preview it.
pub const PREVIEW_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// A local copy of a remote file for previews and thumbnails (up to
/// `PREVIEW_MAX_BYTES`).
pub fn preview_copy(path: &Path) -> Result<PathBuf, String> {
    local_copy(path, known_size(path), PREVIEW_MAX_BYTES)
}

/// A local copy of a remote file, downloaded on first use and kept for
/// this session (for opening, previews and thumbnails). Files over
/// `max_bytes` aren't fetched.
pub fn local_copy(path: &Path, size: Option<u64>, max_bytes: u64) -> Result<PathBuf, String> {
    let (id, segments) = split(path).ok_or("Not a remote path")?;
    let conn = connection(id).ok_or("This network location was removed.")?;
    let name = segments.last().ok_or("Not a file")?;
    let name = safe_local_name(name).ok_or("Unsafe file name")?;
    if size.is_some_and(|s| s > max_bytes) {
        return Err("Too large to preview".into());
    }
    let folder = temp_root()
        .join(id.to_string())
        .join(segments[..segments.len() - 1].join("_"));
    let local = folder.join(name);
    if local.is_file() {
        return Ok(local);
    }
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let remote = remote_path(&segments);
    let target = local.clone();
    run_blocking(&conn, move |fs| {
        download_all(fs, &remote, false, &target, &mut |_| {}, &|| false)
    })?;
    Ok(local)
}

/// Lists a remote folder in the background as file items for the view;
/// `error` gets the reason if it fails.
pub fn list_async(
    path: &Path,
    tx: crossbeam_channel::Sender<crate::core::fs::FileItem>,
    error: std::sync::Arc<Mutex<Option<String>>>,
    dates: (crate::core::fs::DateStyle, bool, String),
    wake: impl Fn() + Send + Clone + 'static,
) {
    let Some((id, segments)) = split(path) else {
        return;
    };
    let Some(conn) = connection(id) else {
        *error.lock().unwrap_or_else(|e| e.into_inner()) =
            Some("This network location was removed.".into());
        return;
    };
    let (error2, wake2) = (error.clone(), wake.clone());
    let dir = remote_path(&segments);
    submit(
        &conn,
        move |fs| {
            match fs.list(&dir) {
                Ok(entries) => {
                    let (style, h24, custom) = &dates;
                    let date = |secs: Option<i64>| {
                        let raw = secs.map(unix_to_filetime);
                        (
                            raw,
                            raw.and_then(|r| {
                                crate::core::fs::filetime_to_string(r, *style, *h24, custom)
                            }),
                        )
                    };
                    let mut sizes = Vec::new();
                    for entry in entries {
                        let mut item_segments = segments.clone();
                        item_segments.push(entry.name.clone());
                        let item_path = path_for(id, &item_segments);
                        if let Some(size) = entry.size {
                            sizes.push((item_path.clone(), size));
                        }
                        let (raw, text) = date(entry.modified);
                        let hidden = entry.name.starts_with('.');
                        let item = crate::core::fs::FileItem::new(
                            entry.name,
                            item_path,
                            entry.is_dir,
                            hidden,
                            None,
                            entry.size,
                            text,
                            None,
                            None,
                            raw,
                            None,
                            None,
                            None,
                        );
                        if tx.send(item).is_err() {
                            break;
                        }
                    }
                    remember_sizes(sizes);
                }
                Err(e) => *error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e),
            }
            drop(tx);
            wake();
        },
        move |e| {
            *error2.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
            wake2();
        },
    );
}

/// Forgets downloaded copies under a remote folder (after it changed).
pub fn forget_local_copies(id: u64) {
    let _ = std::fs::remove_dir_all(temp_root().join(id.to_string()));
}

/// Removes every downloaded copy (at startup).
pub fn clean_temp() {
    let _ = std::fs::remove_dir_all(temp_root());
}

/// Seconds since 1970 → Windows FILETIME ticks (what `FileItem` uses).
pub fn unix_to_filetime(secs: i64) -> i64 {
    (secs + 11_644_473_600) * 10_000_000
}

/// Parses an HTTP date (`Tue, 15 Nov 1994 08:12:31 GMT`) or ISO 8601 date.
pub fn parse_date(s: &str) -> Option<i64> {
    let s = s.trim();
    chrono::DateTime::parse_from_rfc2822(s)
        .or_else(|_| chrono::DateTime::parse_from_rfc3339(s))
        .map(|d| d.timestamp())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_paths_round_trip() {
        let p = path_for(7, &["docs".into(), "a b.txt".into()]);
        assert_eq!(p.to_string_lossy(), r"\\?\EdenRemote\7\docs\a b.txt");
        assert_eq!(split(&p), Some((7, vec!["docs".into(), "a b.txt".into()])));
        assert_eq!(split(&path_for(7, &[])), Some((7, vec![])));
        assert_eq!(split(Path::new(r"C:\docs")), None);
        assert_eq!(remote_path(&["docs".into(), "x".into()]), "/docs/x");
        assert_eq!(remote_path(&[]), "/");
        assert_eq!(segments_of("/home/me/"), vec!["home", "me"]);
        assert_eq!(join("/", "a"), "/a");
        assert_eq!(join("/a", "b"), "/a/b");
        assert_eq!(p.file_name().unwrap(), "a b.txt");
        assert_eq!(p.parent().unwrap(), path_for(7, &["docs".into()]));
    }

    #[test]
    fn server_names_cannot_escape_the_target_folder() {
        assert_eq!(safe_local_name("photo.jpg"), Some("photo.jpg"));
        for bad in ["..", ".", "", "a/b", r"..\x", "c:evil", "a\0"] {
            assert_eq!(safe_local_name(bad), None, "{bad}");
        }
    }

    /// Against local test servers (see the docs): `cargo test -- --ignored live`.
    #[test]
    #[ignore]
    fn live_servers_round_trip() {
        let base = RemoteConnection {
            host: "127.0.0.1".into(),
            username: "user".into(),
            https: false,
            ..Default::default()
        };
        let cases = [
            RemoteConnection {
                id: 9001,
                kind: RemoteKind::Ftp,
                port: 2121,
                ..base.clone()
            },
            RemoteConnection {
                id: 9002,
                kind: RemoteKind::Sftp,
                port: 2222,
                ..base.clone()
            },
            RemoteConnection {
                id: 9003,
                kind: RemoteKind::WebDav,
                port: 8081,
                path: "/dav".into(),
                ..base.clone()
            },
            RemoteConnection {
                id: 9004,
                kind: RemoteKind::S3,
                port: 9000,
                username: "AKTEST".into(),
                bucket: "eden-demo".into(),
                ..base.clone()
            },
        ];
        for conn in cases {
            let secret = if conn.kind == RemoteKind::S3 {
                "secret"
            } else {
                "pass"
            };
            credentials::store(conn.id, secret).unwrap();
            assert_eq!(credentials::load(conn.id).as_deref(), Some(secret));
            let kind = conn.kind;
            let mut fs = connect(&conn).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            let root = fs.list("/").unwrap();
            assert!(
                root.iter().any(|e| e.name == "Reports" && e.is_dir),
                "{kind:?}: {root:?}"
            );
            assert!(
                root.iter()
                    .any(|e| e.name == "readme.md" && e.size == Some(19)),
                "{kind:?}: {root:?}"
            );
            let _ = remove_all(fs.as_mut(), "/Eden Test", true);
            fs.mkdir("/Eden Test").unwrap();
            let data = b"hello from EdenExplorer".repeat(1000);
            let mut seen = 0;
            fs.upload(
                &mut &data[..],
                data.len() as u64,
                "/Eden Test/a b.txt",
                &mut |n| seen = n,
            )
            .unwrap();
            assert_eq!(seen, data.len() as u64, "{kind:?}");
            fs.rename("/Eden Test/a b.txt", "/Eden Test/renamed.txt")
                .unwrap();
            let listed = fs.list("/Eden Test").unwrap();
            assert_eq!(
                listed.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
                ["renamed.txt"],
                "{kind:?}"
            );
            let mut back = Vec::new();
            fs.download("/Eden Test/renamed.txt", &mut back, &mut |_| {})
                .unwrap();
            assert_eq!(back, data, "{kind:?}");
            remove_all(fs.as_mut(), "/Eden Test", true).unwrap();
            assert!(
                !fs.list("/").unwrap().iter().any(|e| e.name == "Eden Test"),
                "{kind:?}"
            );
            credentials::remove(conn.id);
            assert_eq!(credentials::load(conn.id), None);
        }
    }

    #[test]
    fn dates() {
        assert_eq!(parse_date("Tue, 15 Nov 1994 08:12:31 GMT"), Some(784887151));
        assert_eq!(parse_date("2024-01-02T03:04:05.000Z"), Some(1704164645));
    }
}
