//! An interactive shell on a remote server, for the terminal pane.
//!
//! The local terminal runs a shell in a Windows pseudo console and lets
//! `alacritty_terminal`'s own event loop read from it. There is no pseudo
//! console here - the bytes come off an SSH channel instead - so this module
//! replaces just that loop: a thread reads the channel, runs what arrives
//! through the same `vte` parser into the same `Term` grid, and sends
//! keystrokes and window-size changes back the other way. Everything above
//! it (the grid, the renderer, key encoding, selection) is shared with the
//! local terminal and knows nothing about SSH.
//!
//! This opens its own SSH connection rather than borrowing the one
//! `core::remote::sftp` already has: that one lives on a current-thread
//! runtime that only advances inside a `block_on` for each file operation,
//! so a shell sharing it would only receive output while the user happened
//! to be listing a directory. Both sign in through `sftp::sign_in`, so
//! there is still only one auth path.

use crate::core::remote::RemoteConnection;
use crate::core::terminal::{Listener, SharedTerm};
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::vte::ansi::Processor;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// What the UI thread sends to a running shell.
pub enum Msg {
    Input(Vec<u8>),
    /// Columns, rows.
    Resize(u16, u16),
    Shutdown,
}

/// The handle the terminal session keeps. Dropping it ends the shell.
pub struct SshShell {
    tx: UnboundedSender<Msg>,
}

impl SshShell {
    pub fn send(&self, msg: Msg) {
        let _ = self.tx.send(msg);
    }

    /// A send-only handle. Unlike `SshShell` this does **not** end the shell
    /// when dropped, so the reader thread's own reply path can hold one
    /// without a second owner deciding when the shell dies.
    pub fn sender(&self) -> SshSender {
        SshSender(self.tx.clone())
    }
}

#[derive(Clone)]
pub struct SshSender(UnboundedSender<Msg>);

impl SshSender {
    pub fn send(&self, msg: Msg) {
        let _ = self.0.send(msg);
    }
}

impl Drop for SshShell {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Shutdown);
    }
}

/// Starts a shell on `conn` and feeds its output into `term`.
///
/// Returns as soon as the thread is running - signing in takes a moment and
/// must not block the UI, so a failure arrives as text written into the grid
/// rather than as an `Err` from here.
pub fn start(
    conn: &RemoteConnection,
    secret: String,
    cols: u16,
    rows: u16,
    term: SharedTerm,
    listener: Listener,
    connecting: String,
) -> SshShell {
    let (tx, rx) = unbounded_channel();
    let conn = conn.clone();
    // Signing in takes a moment and the grid is otherwise blank, which reads
    // as the pane having failed to open at all.
    report(&term, &listener, &format!("{connecting} {}", conn.host));
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(e) => {
                report(&term, &listener, &format!("Couldn't start the shell: {e}"));
                return;
            }
        };
        if let Err(e) = runtime.block_on(run(&conn, &secret, cols, rows, &term, &listener, rx)) {
            report(&term, &listener, &e);
        }
    });
    SshShell { tx }
}

/// Writes a message into the grid itself. The pane has nowhere else to show
/// one - it is drawn from the grid and nothing more - and a connection
/// failure is exactly what the user needs to see there.
fn report(term: &SharedTerm, listener: &Listener, text: &str) {
    let text = format!("\r\n{text}\r\n");
    let mut parser: Processor = Processor::new();
    {
        let mut term = term.lock();
        parser.advance(&mut *term, text.as_bytes());
    }
    listener.send_event(Event::Wakeup);
}

async fn run(
    conn: &RemoteConnection,
    secret: &str,
    cols: u16,
    rows: u16,
    term: &SharedTerm,
    listener: &Listener,
    mut rx: UnboundedReceiver<Msg>,
) -> Result<(), String> {
    let ssh = crate::core::remote::sftp::sign_in(conn, secret)
        .await
        .map_err(|e| crate::core::remote::sftp::explain(conn, e))?;
    let mut channel = ssh
        .channel_open_session()
        .await
        .map_err(|e| format!("Couldn't open a shell channel: {e}"))?;
    // No pixel size: the server only needs the character grid, and nothing
    // here reports a meaningful pixel size for the remote's benefit.
    channel
        .request_pty(true, TERM, cols.max(2) as u32, rows.max(1) as u32, 0, 0, &[])
        .await
        .map_err(|e| format!("The server refused a terminal: {e}"))?;
    channel
        .request_shell(true)
        .await
        .map_err(|e| format!("The server refused a shell: {e}"))?;

    let mut parser: Processor = Processor::new();
    loop {
        tokio::select! {
            // Biased so a shutdown is acted on even while output is still
            // arriving - otherwise closing a tab mid-`cat` could wait.
            biased;
            msg = rx.recv() => match msg {
                Some(Msg::Input(bytes)) => {
                    if channel.data_bytes(bytes).await.is_err() {
                        break;
                    }
                }
                Some(Msg::Resize(cols, rows)) => {
                    let _ = channel
                        .window_change(cols.max(2) as u32, rows.max(1) as u32, 0, 0)
                        .await;
                }
                // The handle was dropped, or the pane asked to close.
                Some(Msg::Shutdown) | None => break,
            },
            incoming = channel.wait() => match incoming {
                Some(russh::ChannelMsg::Data { data }) => {
                    {
                        let mut term = term.lock();
                        parser.advance(&mut *term, &data);
                    }
                    listener.send_event(Event::Wakeup);
                }
                // stderr on a shell channel is rare (a PTY merges it), but
                // a server that sends it anyway should still be readable.
                Some(russh::ChannelMsg::ExtendedData { data, .. }) => {
                    {
                        let mut term = term.lock();
                        parser.advance(&mut *term, &data);
                    }
                    listener.send_event(Event::Wakeup);
                }
                Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                    use std::os::windows::process::ExitStatusExt;
                    listener.send_event(Event::ChildExit(std::process::ExitStatus::from_raw(
                        exit_status,
                    )));
                    break;
                }
                Some(russh::ChannelMsg::Eof) | Some(russh::ChannelMsg::Close) | None => break,
                Some(_) => {}
            },
        }
    }
    let _ = channel.close().await;
    listener.send_event(Event::Exit);
    Ok(())
}

/// What the remote shell is told it is talking to. The grid is a real
/// `alacritty_terminal`, so claiming xterm-256color is accurate enough for
/// colour and cursor handling.
const TERM: &str = "xterm-256color";

/// `cd` into `dir` on the remote, for Follow Folder.
///
/// Single-quoted because a remote path can hold spaces and shell
/// metacharacters, with embedded single quotes closed and reopened the usual
/// way - this is typed into a live shell, so getting it wrong would run
/// something the user never asked for. Leads with Ctrl+U to clear whatever
/// the user had half-typed, and ends with a carriage return, matching what
/// `ShellProfile::cd_command` already sends every other bash-like shell.
pub fn cd_command(dir: &str) -> String {
    format!("\x15cd '{}'\r", dir.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_path_is_quoted_and_clears_the_line_first() {
        assert_eq!(cd_command("/var/log"), "\x15cd '/var/log'\r");
    }

    #[test]
    fn spaces_and_metacharacters_stay_inside_the_quotes() {
        assert_eq!(
            cd_command("/home/me/my stuff; rm -rf /"),
            "\x15cd '/home/me/my stuff; rm -rf /'\r"
        );
    }

    #[test]
    fn an_embedded_single_quote_cannot_end_the_quoting() {
        // The dangerous case: a folder literally named `'; rm -rf /; '`
        // must not be able to close the quote and run a second command.
        let cmd = cd_command("/tmp/it's here");
        assert_eq!(cmd, "\x15cd '/tmp/it'\\''s here'\r");
        assert!(!cmd.contains("' "), "no bare quote may end the argument early");
    }
}
