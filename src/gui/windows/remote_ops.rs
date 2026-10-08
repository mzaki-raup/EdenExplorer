//! The main window's side of remote locations: what file actions do in a
//! remote folder (open, new folder, rename, delete, copy/cut/paste, drag and
//! drop, Download To…), each run on the connection's worker as a job with a
//! notification (progress and Cancel), then reloading the folders it
//! changed. Things that make no sense on a server (shortcuts, compress, tags'
//! Send To, ...) are ignored there.

use crate::core::remote::{self, RemoteConnection, RemoteFs};
use crate::gui::windows::containers::enums::{ItemViewerAction, ItemViewerContextAction};
use crate::gui::windows::containers::notifications::{FileOpKind, FileOpStatus};
use crate::gui::windows::containers::structs::RenameState;
use crate::gui::windows::mainwindow::MainWindow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

pub enum RemoteEvent {
    Progress(u64, u64),
    Done(Result<(), String>),
}

/// What to do once a job succeeds.
pub enum RemoteAfter {
    None,
    /// Open this downloaded file with its default app.
    OpenLocal(PathBuf),
    /// Start renaming this new folder.
    RenameNew(PathBuf),
}

pub struct RemoteJob {
    rx: Receiver<RemoteEvent>,
    /// Quick jobs (new folder, rename, opening a small file) show a notification only on failure.
    silent: Option<(FileOpKind, usize, String)>,
    refresh: Vec<PathBuf>,
    after: RemoteAfter,
    shown: f32,
}

/// Remote items copied or cut in the app (the system clipboard can't hold
/// them: other apps couldn't read them).
pub struct RemoteClipboard {
    /// (path, is folder, size)
    items: Vec<(PathBuf, bool, Option<u64>)>,
    cut: bool,
    /// The system clipboard's sequence number when this was set: if anything
    /// was copied since, that wins.
    sequence: u32,
}

/// Items waiting for the user to confirm deleting them from the server.
pub struct PendingRemoteDelete {
    items: Vec<(PathBuf, bool)>,
}

type Work = Box<
    dyn FnOnce(&mut dyn RemoteFs, &mut dyn FnMut(u64, u64), &dyn Fn() -> bool) -> Result<(), String>
        + Send,
>;

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// A folder's name for notifications (a location's top folder: its name).
/// Label marking a job as quiet (see `start_remote_job`).
const QUIET: &str = "\0quiet";
/// Files below this open without a progress notification.
const QUICK_OPEN_BYTES: u64 = 8 * 1024 * 1024;

fn folder_label(path: &Path) -> String {
    match remote::split(path) {
        Some((id, segments)) if segments.is_empty() => remote::connection(id)
            .map(|c| c.display_name())
            .unwrap_or_default(),
        _ => name_of(path),
    }
}

/// The remote path (`/a/b`) of a remote item.
fn rpath(path: &Path) -> String {
    remote::split(path)
        .map(|(_, s)| remote::remote_path(&s))
        .unwrap_or_default()
}

/// A name for `name` in `dir` that isn't taken (`a.txt` → `a (2).txt`).
fn unique_local(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(candidate)
}

impl MainWindow {
    fn remote_conn(&self, path: &Path) -> Option<RemoteConnection> {
        remote::connection_of(path)
    }

    /// (is folder, size) of a listed remote item in the focused view.
    fn remote_info(&self, path: &Path) -> (bool, Option<u64>) {
        let view = self.active_tab().view(self.focused_split);
        view.files
            .iter()
            .find(|f| f.path == path)
            .map(|f| (f.is_dir, f.file_size))
            .unwrap_or((false, None))
    }

    #[allow(clippy::too_many_arguments)]
    fn start_remote_job(
        &mut self,
        conn: &RemoteConnection,
        kind: FileOpKind,
        items: usize,
        label: String,
        refresh: Vec<PathBuf>,
        after: RemoteAfter,
        work: Work,
    ) {
        // Quick jobs (renames, a new folder, opening a small file) only
        // report failures, so they don't pop up a notification each time.
        let silent = matches!(kind, FileOpKind::Rename)
            || matches!(after, RemoteAfter::RenameNew(_))
            || label == QUIET;
        let cancel = Arc::new(AtomicBool::new(false));
        let id = if silent {
            self.next_silent_remote_job = self.next_silent_remote_job.wrapping_sub(1);
            self.next_silent_remote_job
        } else {
            let id = self.notifications_state.start_operation(
                kind,
                items,
                label.clone(),
                self.settings_window
                    .current_settings
                    .auto_open_notification_panel,
            );
            self.notifications_state
                .attach_cancel_flag(id, cancel.clone());
            id
        };
        let (tx, rx) = channel();
        let (tx2, ctx, ctx2) = (tx.clone(), self.egui_ctx.clone(), self.egui_ctx.clone());
        remote::submit(
            conn,
            move |fs| {
                let mut last = Instant::now();
                let mut progress = |done: u64, total: u64| {
                    if last.elapsed().as_millis() >= 100 {
                        last = Instant::now();
                        let _ = tx.send(RemoteEvent::Progress(done, total));
                        if let Some(ctx) = &ctx {
                            ctx.request_repaint();
                        }
                    }
                };
                let result = work(fs, &mut progress, &|| cancel.load(Ordering::Relaxed));
                let _ = tx.send(RemoteEvent::Done(result));
                if let Some(ctx) = &ctx {
                    ctx.request_repaint();
                }
            },
            move |e| {
                let _ = tx2.send(RemoteEvent::Done(Err(e)));
                if let Some(ctx) = &ctx2 {
                    ctx.request_repaint();
                }
            },
        );
        let label = if label == QUIET { String::new() } else { label };
        let silent = silent.then_some((kind, items, label));
        self.remote_jobs.insert(
            id,
            RemoteJob {
                rx,
                silent,
                refresh,
                after,
                shown: -1.0,
            },
        );
    }

    /// Each frame: progress, and finishing jobs (reloading the folders they
    /// changed).
    pub(crate) fn poll_remote_jobs(&mut self) {
        if self.remote_jobs.is_empty() {
            return;
        }
        let mut finished = Vec::new();
        for (&id, job) in self.remote_jobs.iter_mut() {
            while let Ok(event) = job.rx.try_recv() {
                match event {
                    RemoteEvent::Progress(done, total) => {
                        let p = if total > 0 {
                            (done as f32 / total as f32).min(1.0)
                        } else {
                            0.0
                        };
                        if job.silent.is_none() && (p - job.shown).abs() >= 0.01 {
                            job.shown = p;
                            self.notifications_state.set_progress(id, Some(p));
                        }
                    }
                    RemoteEvent::Done(result) => finished.push((id, result)),
                }
            }
        }
        for (id, result) in finished {
            let Some(job) = self.remote_jobs.remove(&id) else {
                continue;
            };
            match (&job.silent, &result) {
                (Some(_), Ok(())) => {}
                (Some((kind, items, label)), Err(e)) => {
                    let auto_open = self
                        .settings_window
                        .current_settings
                        .auto_open_notification_panel;
                    let failed = self.notifications_state.record_finished(
                        *kind,
                        *items,
                        label.clone(),
                        FileOpStatus::Failed,
                        auto_open,
                    );
                    self.notifications_state.set_detail(failed, Some(e.clone()));
                }
                (None, _) => {
                    self.notifications_state.detach_cancel_flag(id);
                    let status = match &result {
                        Ok(()) => FileOpStatus::Completed,
                        Err(e) if e == "Cancelled" => FileOpStatus::Cancelled,
                        Err(e) => {
                            self.notifications_state.set_detail(id, Some(e.clone()));
                            FileOpStatus::Failed
                        }
                    };
                    self.notifications_state.finish_operation(id, status);
                }
            }
            for dir in &job.refresh {
                if let Some((conn, _)) = remote::split(dir) {
                    remote::forget_local_copies(conn);
                }
            }
            self.reload_views_showing(&job.refresh);
            if result.is_ok() {
                match job.after {
                    RemoteAfter::None => {}
                    RemoteAfter::OpenLocal(local) => {
                        crate::gui::windows::mainwindow_imp::handle_pending_actions(
                            Some(ItemViewerAction::OpenWithDefault(vec![local])),
                            self,
                        )
                    }
                    RemoteAfter::RenameNew(path) => {
                        let side = self.focused_split;
                        self.rename_state = Some(RenameState {
                            path: path.clone(),
                            new_name: name_of(&path),
                            should_focus: true,
                            validation_error_show: false,
                        });
                        self.active_tab_mut()
                            .view_mut(side)
                            .explorer_state
                            .pending_selection_paths = Some(vec![path]);
                    }
                }
            }
        }
    }

    /// Reloads the active tab's panes showing any of `dirs`.
    fn reload_views_showing(&mut self, dirs: &[PathBuf]) {
        use crate::gui::windows::containers::structs::SplitSide;
        let restore = self.focused_split;
        for side in [SplitSide::Primary, SplitSide::Secondary] {
            if side == SplitSide::Secondary && self.active_tab().split_view.is_none() {
                continue;
            }
            if dirs.contains(&self.active_tab().view(side).nav.current) {
                self.focused_split = side;
                self.load_view(side);
            }
        }
        self.focused_split = restore;
    }

    /// Downloads remote items into the local folder `target` (`delete_after`:
    /// a cut, so the originals are removed once copied).
    fn download_remote(
        &mut self,
        items: Vec<(PathBuf, bool, Option<u64>)>,
        target: PathBuf,
        delete_after: bool,
    ) {
        let Some(conn) = items.first().and_then(|(p, ..)| self.remote_conn(p)) else {
            return;
        };
        let label = name_of(&target);
        let mut refresh = vec![target.clone()];
        if delete_after && let Some(parent) = items[0].0.parent() {
            refresh.push(parent.to_path_buf());
        }
        let count = items.len();
        let kind = if delete_after {
            FileOpKind::Move
        } else {
            FileOpKind::Copy
        };
        self.start_remote_job(
            &conn,
            kind,
            count,
            label,
            refresh,
            RemoteAfter::None,
            Box::new(move |fs, progress, cancel| {
                let total: u64 = items
                    .iter()
                    .map(|(p, dir, size)| remote::remote_size(fs, &rpath(p), *size, *dir))
                    .sum();
                let mut base = 0u64;
                for (path, is_dir, _) in &items {
                    let name = name_of(path);
                    let name = remote::safe_local_name(&name).ok_or("Unsafe file name")?;
                    let local = unique_local(&target, name);
                    let mut last = 0u64;
                    let mut file_progress = |done: u64| {
                        // Folders report per file; keep a running total.
                        if done < last {
                            base += last;
                        }
                        last = done;
                        progress(base + done, total);
                    };
                    remote::download_all(
                        fs,
                        &rpath(path),
                        *is_dir,
                        &local,
                        &mut file_progress,
                        cancel,
                    )?;
                    base += last;
                    if delete_after {
                        remote::remove_all(fs, &rpath(path), *is_dir)?;
                    }
                }
                Ok(())
            }),
        );
    }

    /// Uploads local files and folders into the remote folder `target`.
    fn upload_local(&mut self, sources: Vec<PathBuf>, target: PathBuf) {
        let Some(conn) = self.remote_conn(&target) else {
            return;
        };
        let existing: Vec<String> = self
            .active_tab()
            .view(self.focused_split)
            .files
            .iter()
            .map(|f| f.name.clone())
            .collect();
        let label = folder_label(&target);
        let count = sources.len();
        let dir = rpath(&target);
        self.start_remote_job(
            &conn,
            FileOpKind::Copy,
            count,
            label,
            vec![target],
            RemoteAfter::None,
            Box::new(move |fs, progress, cancel| {
                let total: u64 = sources.iter().map(|p| remote::local_size(p)).sum();
                let mut base = 0u64;
                for source in &sources {
                    let name = name_of(source);
                    // Don't overwrite: pick a free name, as for local copies.
                    let mut target_name = name.clone();
                    let (stem, ext) = match name.rsplit_once('.') {
                        Some((s, e)) if !s.is_empty() && !source.is_dir() => {
                            (s.to_string(), format!(".{e}"))
                        }
                        _ => (name.clone(), String::new()),
                    };
                    let mut n = 2;
                    while existing
                        .iter()
                        .any(|e| e.eq_ignore_ascii_case(&target_name))
                    {
                        target_name = format!("{stem} ({n}){ext}");
                        n += 1;
                    }
                    let mut last = 0u64;
                    let mut file_progress = |done: u64| {
                        if done < last {
                            base += last;
                        }
                        last = done;
                        progress(base + done, total);
                    };
                    remote::upload_all(
                        fs,
                        source,
                        &remote::join(&dir, &target_name),
                        &mut file_progress,
                        cancel,
                    )?;
                    base += last;
                }
                Ok(())
            }),
        );
    }

    /// Moves remote items into another folder of the same location.
    fn move_remote(&mut self, items: Vec<PathBuf>, target: PathBuf) {
        let Some(conn) = self.remote_conn(&target) else {
            return;
        };
        let mut refresh = vec![target.clone()];
        refresh.extend(
            items
                .iter()
                .filter_map(|p| p.parent().map(Path::to_path_buf)),
        );
        let dir = rpath(&target);
        let count = items.len();
        self.start_remote_job(
            &conn,
            FileOpKind::Move,
            count,
            folder_label(&target),
            refresh,
            RemoteAfter::None,
            Box::new(move |fs, _, _| {
                for item in &items {
                    if item.parent() == Some(target.as_path()) {
                        continue;
                    }
                    fs.rename(&rpath(item), &remote::join(&dir, &name_of(item)))?;
                }
                Ok(())
            }),
        );
    }

    /// Copies remote items into another folder of the same location (down
    /// and back up: servers don't copy by themselves).
    fn copy_remote_within(&mut self, items: Vec<(PathBuf, bool, Option<u64>)>, target: PathBuf) {
        let Some(conn) = self.remote_conn(&target) else {
            return;
        };
        let dir = rpath(&target);
        let existing: Vec<String> = self
            .active_tab()
            .view(self.focused_split)
            .files
            .iter()
            .map(|f| f.name.clone())
            .collect();
        let count = items.len();
        self.start_remote_job(
            &conn,
            FileOpKind::Copy,
            count,
            folder_label(&target),
            vec![target.clone()],
            RemoteAfter::None,
            Box::new(move |fs, progress, cancel| {
                let temp = remote::temp_root()
                    .join("copy")
                    .join(format!("{}", std::process::id()));
                let _ = std::fs::remove_dir_all(&temp);
                std::fs::create_dir_all(&temp).map_err(|e| e.to_string())?;
                let result = (|| {
                    for (path, is_dir, _) in &items {
                        let name = name_of(path);
                        let local =
                            temp.join(remote::safe_local_name(&name).ok_or("Unsafe file name")?);
                        remote::download_all(
                            fs,
                            &rpath(path),
                            *is_dir,
                            &local,
                            &mut |_| {},
                            cancel,
                        )?;
                        let mut target_name = name.clone();
                        let mut n = 2;
                        while existing
                            .iter()
                            .any(|e| e.eq_ignore_ascii_case(&target_name))
                        {
                            target_name = format!("{name} ({n})");
                            n += 1;
                        }
                        remote::upload_all(
                            fs,
                            &local,
                            &remote::join(&dir, &target_name),
                            &mut |d| progress(d, 0),
                            cancel,
                        )?;
                    }
                    Ok(())
                })();
                let _ = std::fs::remove_dir_all(&temp);
                result
            }),
        );
    }

    /// Downloads a remote file to a temporary folder and opens it.
    fn open_remote_file(&mut self, path: PathBuf) {
        let Some(conn) = self.remote_conn(&path) else {
            return;
        };
        let (_, size) = self.remote_info(&path);
        let name = name_of(&path);
        let Some(safe) = remote::safe_local_name(&name) else {
            return;
        };
        let folder = remote::temp_root().join("open").join(format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
        let local = folder.join(safe);
        let target = local.clone();
        let source = rpath(&path);
        let label = if size.is_some_and(|s| s < QUICK_OPEN_BYTES) {
            QUIET.to_string()
        } else {
            String::new()
        };
        self.start_remote_job(
            &conn,
            FileOpKind::Copy,
            1,
            label,
            Vec::new(),
            RemoteAfter::OpenLocal(local),
            Box::new(move |fs, progress, cancel| {
                std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                remote::download_all(
                    fs,
                    &source,
                    false,
                    &target,
                    &mut |d| progress(d, size.unwrap_or(0)),
                    cancel,
                )
            }),
        );
    }

    /// Download To… for remote items: asks for a folder.
    pub(crate) fn download_remote_to(&mut self, paths: Vec<PathBuf>) {
        let Some(target) = crate::gui::windows::windowsoverrides::dialog()
            .set_title(self.i18n.tr("remote_download_to"))
            .pick_folder()
        else {
            return;
        };
        let items = paths
            .iter()
            .map(|p| {
                let (dir, size) = self.remote_info(p);
                (p.clone(), dir, size)
            })
            .collect();
        self.download_remote(items, target, false);
    }

    /// Handles an action that involves a remote location. Returns the action
    /// back when it isn't one (for the normal handling).
    pub(crate) fn handle_remote_action(
        &mut self,
        action: ItemViewerAction,
    ) -> Option<ItemViewerAction> {
        let current = self.current_nav().current.clone();
        let here_remote = remote::is_remote(&current);
        let any_remote = |paths: &[PathBuf]| paths.iter().any(|p| remote::is_remote(p));
        let cut = matches!(
            &action,
            ItemViewerAction::Context(ItemViewerContextAction::Cut(_))
        );
        match action {
            ItemViewerAction::OpenWithDefault(paths) if any_remote(&paths) => {
                for path in paths.into_iter().filter(|p| remote::is_remote(p)) {
                    if self.remote_info(&path).0 {
                        self.open_path_in_current_view(path);
                    } else {
                        self.open_remote_file(path);
                    }
                }
                None
            }
            ItemViewerAction::RemoteDownloadTo(paths) => {
                self.download_remote_to(paths);
                None
            }
            ItemViewerAction::CreateFolder if here_remote => {
                let conn = self.remote_conn(&current)?;
                let names: Vec<String> = self
                    .active_tab()
                    .view(self.focused_split)
                    .files
                    .iter()
                    .map(|f| f.name.to_lowercase())
                    .collect();
                let mut name = "New Folder".to_string();
                let mut n = 2;
                while names.contains(&name.to_lowercase()) {
                    name = format!("New Folder ({n})");
                    n += 1;
                }
                let new_path = current.join(&name);
                let dir = remote::join(&rpath(&current), &name);
                self.start_remote_job(
                    &conn,
                    FileOpKind::Copy,
                    1,
                    name,
                    vec![current],
                    RemoteAfter::RenameNew(new_path),
                    Box::new(move |fs, _, _| fs.mkdir(&dir)),
                );
                None
            }
            ItemViewerAction::Context(ItemViewerContextAction::RenameRequest(path, new_name))
                if remote::is_remote(&path) =>
            {
                self.rename_state = None;
                let new_name = new_name.trim().to_string();
                if new_name.is_empty()
                    || new_name.contains(['/', '\\'])
                    || new_name == name_of(&path)
                {
                    return None;
                }
                let conn = self.remote_conn(&path)?;
                let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
                let (from, to) = (rpath(&path), remote::join(&rpath(&parent), &new_name));
                self.start_remote_job(
                    &conn,
                    FileOpKind::Rename,
                    1,
                    new_name,
                    vec![parent],
                    RemoteAfter::None,
                    Box::new(move |fs, _, _| fs.rename(&from, &to)),
                );
                None
            }
            ItemViewerAction::Context(ItemViewerContextAction::Delete(paths, _))
                if any_remote(&paths) =>
            {
                let items = paths
                    .iter()
                    .filter(|p| remote::is_remote(p))
                    .map(|p| (p.clone(), self.remote_info(p).0))
                    .collect();
                self.pending_remote_delete = Some(PendingRemoteDelete { items });
                None
            }
            ItemViewerAction::Context(
                ItemViewerContextAction::Copy(paths) | ItemViewerContextAction::Cut(paths),
            ) if any_remote(&paths) => {
                let urls: Vec<String> = paths.iter().filter_map(|p| remote::url_for(p)).collect();
                crate::core::utils::clipboard::copy_text_to_clipboard(&urls.join("\r\n"));
                let items = paths
                    .iter()
                    .map(|p| {
                        let (dir, size) = self.remote_info(p);
                        (p.clone(), dir, size)
                    })
                    .collect();
                self.remote_clipboard = Some(RemoteClipboard {
                    items,
                    cut,
                    sequence: crate::core::utils::clipboard::clipboard_sequence(),
                });
                None
            }
            ItemViewerAction::Context(ItemViewerContextAction::CopyPath(paths))
                if any_remote(&paths) =>
            {
                let urls: Vec<String> = paths.iter().filter_map(|p| remote::url_for(p)).collect();
                crate::core::utils::clipboard::copy_text_to_clipboard(&urls.join("\r\n"));
                None
            }
            ItemViewerAction::Context(ItemViewerContextAction::Paste) => {
                let clip = self
                    .remote_clipboard
                    .take_if(|c| c.sequence == crate::core::utils::clipboard::clipboard_sequence());
                match clip {
                    Some(clip) if here_remote => {
                        let same = clip
                            .items
                            .first()
                            .and_then(|(p, ..)| remote::split(p))
                            .map(|(id, _)| id)
                            == remote::split(&current).map(|(id, _)| id);
                        if !same {
                            self.notifications_state.record_finished(
                                FileOpKind::Copy,
                                clip.items.len(),
                                self.i18n.tr("remote_between_locations"),
                                FileOpStatus::Failed,
                                true,
                            );
                            return None;
                        }
                        if clip.cut {
                            self.move_remote(
                                clip.items.into_iter().map(|(p, ..)| p).collect(),
                                current,
                            );
                        } else {
                            self.copy_remote_within(clip.items, current);
                        }
                        None
                    }
                    Some(clip) => {
                        if crate::core::launch::is_virtual_path(&current) || !current.is_dir() {
                            return None;
                        }
                        let cut = clip.cut;
                        self.download_remote(clip.items, current, cut);
                        None
                    }
                    None if here_remote => {
                        if let Some(files) = crate::core::utils::clipboard::get_clipboard_files()
                            && !files.is_empty()
                        {
                            self.upload_local(files, current);
                        }
                        None
                    }
                    None => Some(ItemViewerAction::Context(ItemViewerContextAction::Paste)),
                }
            }
            ItemViewerAction::FilesDropped(paths) if here_remote => {
                let local: Vec<PathBuf> = paths
                    .into_iter()
                    .filter(|p| !remote::is_remote(p))
                    .collect();
                if !local.is_empty() {
                    self.upload_local(local, current);
                }
                None
            }
            ItemViewerAction::MoveItems {
                sources,
                target_dir,
            } if remote::is_remote(&target_dir) || any_remote(&sources) => {
                match (
                    remote::split(&target_dir),
                    sources.first().and_then(|p| remote::split(p)),
                ) {
                    // Local → remote: upload (a copy; the originals stay).
                    (Some(_), None) => self.upload_local(sources, target_dir),
                    // Remote → local: download.
                    (None, Some(_)) => {
                        let items = sources
                            .iter()
                            .map(|p| {
                                let (dir, size) = self.remote_info(p);
                                (p.clone(), dir, size)
                            })
                            .collect();
                        if !crate::core::launch::is_virtual_path(&target_dir) {
                            self.download_remote(items, target_dir, false);
                        }
                    }
                    // Within one location: move.
                    (Some((a, _)), Some((b, _))) if a == b => self.move_remote(sources, target_dir),
                    _ => {}
                }
                None
            }
            // Nothing else changes a remote folder (no shortcuts, zips,
            // templates, tags' Send To, ...).
            ItemViewerAction::CreateFileFromTemplate(_) | ItemViewerAction::CreateShortcutHere
                if here_remote =>
            {
                None
            }
            ItemViewerAction::Context(
                ItemViewerContextAction::Compress(paths)
                | ItemViewerContextAction::CreateShortcut(paths)
                | ItemViewerContextAction::Properties(paths)
                | ItemViewerContextAction::BulkRenameRequest(paths)
                | ItemViewerContextAction::AddFavorite(paths),
            ) if any_remote(&paths) => None,
            ItemViewerAction::Context(ItemViewerContextAction::Checksum(path))
                if remote::is_remote(&path) =>
            {
                None
            }
            other => Some(other),
        }
    }

    /// "Delete from the server?" confirmation.
    pub(crate) fn draw_remote_delete_confirm(
        &mut self,
        ctx: &egui::Context,
        palette: &crate::gui::theme::ThemePalette,
    ) {
        use crate::core::utils::widgets::{
            eden_button, modal_frame, modal_icon_header, primary_dialog_button,
        };
        use egui_phosphor::regular;
        let Some(pending) = self.pending_remote_delete.take() else {
            return;
        };
        let mut confirmed = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Area::new(egui::Id::new("remote_delete_scrim"))
            .order(egui::Order::Middle)
            .interactable(true)
            .show(ctx, |ui| {
                let rect = ctx.content_rect();
                ui.painter()
                    .rect_filled(rect, 0.0, palette.modal_background_effect_color);
                ui.interact(rect, ui.id().with("block"), egui::Sense::click());
            });
        egui::Area::new(egui::Id::new("remote_delete_area"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                modal_frame(&ctx.style_of(ctx.theme()), palette).show(ui, |ui| {
                    ui.set_width(420.0);
                    let detail = if pending.items.len() == 1 {
                        name_of(&pending.items[0].0)
                    } else {
                        format!("{} {}", pending.items.len(), self.i18n.tr("items_capital"))
                    };
                    modal_icon_header(
                        ui,
                        palette,
                        regular::TRASH,
                        ui.visuals().warn_fg_color,
                        &self.i18n.tr("remote_delete_title"),
                        Some(&format!(
                            "{detail}\n\n{}",
                            self.i18n.tr("remote_delete_hint")
                        )),
                    );
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if primary_dialog_button(ui, palette, &self.i18n.tr("delete")).clicked() {
                            confirmed = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if eden_button(ui, palette, &self.i18n.tr("cancel")).clicked() {
                                close = true;
                            }
                        });
                    });
                });
            });
        if confirmed {
            let Some(conn) = pending.items.first().and_then(|(p, _)| self.remote_conn(p)) else {
                return;
            };
            let refresh: Vec<PathBuf> = pending
                .items
                .iter()
                .filter_map(|(p, _)| p.parent().map(Path::to_path_buf))
                .collect();
            let count = pending.items.len();
            let items = pending.items;
            self.start_remote_job(
                &conn,
                FileOpKind::Delete,
                count,
                String::new(),
                refresh,
                RemoteAfter::None,
                Box::new(move |fs, progress, cancel| {
                    for (i, (path, is_dir)) in items.iter().enumerate() {
                        if cancel() {
                            return Err("Cancelled".into());
                        }
                        remote::remove_all(fs, &rpath(path), *is_dir)?;
                        progress(i as u64 + 1, items.len() as u64);
                    }
                    Ok(())
                }),
            );
        } else if !close {
            self.pending_remote_delete = Some(pending);
        }
    }
}
