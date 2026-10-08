//! SFTP (over SSH) with `russh`, a pure-Rust SSH (modern key exchanges and
//! ed25519/ECDSA/RSA keys, the same on every Windows version). Signs in with
//! a password or a private key file; the server's host key is checked
//! against the one seen on the first connection (`credentials`). The async
//! client runs on a small runtime owned by the connection's worker thread.

use super::{RemoteConnection, RemoteEntry, RemoteFs};
use russh::client;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh_sftp::client::SftpSession;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct SftpFs {
    runtime: tokio::runtime::Runtime,
    // Kept alive alongside its SFTP channel.
    _ssh: client::Handle<HostKeyCheck>,
    sftp: SftpSession,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// A TCP connection with sensible timeouts (also used by FTP).
pub(super) fn tcp(host: &str, port: u16) -> Result<TcpStream, String> {
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("Couldn't find {host}: {e}"))?
        .next()
        .ok_or_else(|| format!("Couldn't find {host}"))?;
    let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(15))
        .map_err(|e| format!("Couldn't connect to {host}:{port}: {e}"))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(60)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(60)));
    Ok(stream)
}

/// Accepts the server only if its host key matches the remembered one.
pub struct HostKeyCheck {
    host: String,
    port: u16,
}

impl client::Handler for HostKeyCheck {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let public = match key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            PublicKeyOrCertificate::Certificate(cert) => cert.public_key().clone().into(),
        };
        let fingerprint = public.fingerprint(HashAlg::Sha256).to_string();
        Ok(super::credentials::check_host_key(&self.host, self.port, &fingerprint).is_ok())
    }
}

impl SftpFs {
    pub fn connect(conn: &RemoteConnection, secret: &str) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(err)?;
        let stream = tcp(&conn.host, conn.port)?;
        stream.set_nonblocking(true).map_err(err)?;
        let (ssh, sftp) = runtime
            .block_on(async {
                let stream = tokio::net::TcpStream::from_std(stream).map_err(err)?;
                let config = Arc::new(client::Config {
                    inactivity_timeout: Some(Duration::from_secs(600)),
                    keepalive_interval: Some(Duration::from_secs(30)),
                    ..Default::default()
                });
                let check = HostKeyCheck {
                    host: conn.host.clone(),
                    port: conn.port,
                };
                let mut ssh = match client::connect_stream(config, stream, check).await {
                    Ok(ssh) => ssh,
                    Err(e) => return Err(format!("SSH connection failed: {e}")),
                };
                let signed_in = match &conn.key_file {
                    Some(key_file) => {
                        let key = russh::keys::load_secret_key(
                            key_file,
                            (!secret.is_empty()).then_some(secret),
                        )
                        .map_err(|e| format!("Couldn't read the key file: {e}"))?;
                        let hash = ssh.best_supported_rsa_hash().await.map_err(err)?.flatten();
                        ssh.authenticate_publickey(
                            &conn.username,
                            PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                        )
                        .await
                        .map_err(err)?
                    }
                    None => ssh
                        .authenticate_password(&conn.username, secret)
                        .await
                        .map_err(err)?,
                };
                if !signed_in.success() {
                    return Err("Signing in failed (wrong user name, password or key).".to_string());
                }
                let channel = ssh.channel_open_session().await.map_err(err)?;
                channel.request_subsystem(true, "sftp").await.map_err(err)?;
                let sftp = SftpSession::new(channel.into_stream())
                    .await
                    .map_err(|e| format!("The server doesn't offer SFTP: {e}"))?;
                Ok((ssh, sftp))
            })
            // A rejected host key surfaces as a generic error; say why.
            .map_err(|e: String| {
                super::credentials::host_key_problem(&conn.host, conn.port).unwrap_or(e)
            })?;
        Ok(Self {
            runtime,
            _ssh: ssh,
            sftp,
        })
    }
}

impl RemoteFs for SftpFs {
    fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>, String> {
        let sftp = &self.sftp;
        self.runtime.block_on(async {
            let entries = sftp.read_dir(dir).await.map_err(err)?;
            let mut out = Vec::new();
            for entry in entries {
                let name = entry.file_name();
                if name == "." || name == ".." {
                    continue;
                }
                let meta = entry.metadata();
                let file_type = entry.file_type();
                // A link to a folder: follow it to tell.
                let is_dir = file_type.is_dir()
                    || (file_type.is_symlink()
                        && sftp
                            .metadata(super::join(dir, &name))
                            .await
                            .is_ok_and(|m| m.is_dir()));
                out.push(RemoteEntry {
                    name,
                    is_dir,
                    size: (!is_dir).then_some(meta.size).flatten(),
                    modified: meta.mtime.map(i64::from),
                });
            }
            Ok(out)
        })
    }

    fn download(
        &mut self,
        file: &str,
        to: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), String> {
        let sftp = &self.sftp;
        self.runtime.block_on(async {
            let mut remote = sftp.open(file).await.map_err(err)?;
            let mut buf = vec![0u8; 256 * 1024];
            let mut done = 0u64;
            loop {
                let n = remote.read(&mut buf).await.map_err(err)?;
                if n == 0 {
                    break;
                }
                to.write_all(&buf[..n]).map_err(err)?;
                done += n as u64;
                progress(done);
            }
            to.flush().map_err(err)
        })
    }

    fn upload(
        &mut self,
        from: &mut dyn Read,
        _size: u64,
        file: &str,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), String> {
        let sftp = &self.sftp;
        self.runtime.block_on(async {
            let mut remote = sftp.create(file).await.map_err(err)?;
            let mut buf = vec![0u8; 256 * 1024];
            let mut done = 0u64;
            loop {
                let n = from.read(&mut buf).map_err(err)?;
                if n == 0 {
                    break;
                }
                remote.write_all(&buf[..n]).await.map_err(err)?;
                done += n as u64;
                progress(done);
            }
            remote.shutdown().await.map_err(err)
        })
    }

    fn mkdir(&mut self, dir: &str) -> Result<(), String> {
        self.runtime
            .block_on(self.sftp.create_dir(dir))
            .map_err(err)
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        self.runtime
            .block_on(self.sftp.rename(from, to))
            .map_err(err)
    }

    fn remove_file(&mut self, file: &str) -> Result<(), String> {
        self.runtime
            .block_on(self.sftp.remove_file(file))
            .map_err(err)
    }

    fn remove_dir(&mut self, dir: &str) -> Result<(), String> {
        self.runtime
            .block_on(self.sftp.remove_dir(dir))
            .map_err(err)
    }
}

/// Copies with progress (bytes done so far).
pub(super) fn copy(
    from: &mut dyn Read,
    to: &mut dyn Write,
    progress: &mut dyn FnMut(u64),
) -> Result<(), String> {
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let n = from.read(&mut buf).map_err(err)?;
        if n == 0 {
            break;
        }
        to.write_all(&buf[..n]).map_err(err)?;
        done += n as u64;
        progress(done);
    }
    to.flush().map_err(err)
}

/// Standard base64 without padding.
pub(super) fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | (*b as u32) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_without_padding() {
        assert_eq!(super::base64(b"Man"), "TWFu");
        assert_eq!(super::base64(b"Ma"), "TWE");
        assert_eq!(super::base64(b"M"), "TQ");
    }
}
