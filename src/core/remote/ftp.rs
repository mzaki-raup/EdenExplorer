//! FTP and FTPS (explicit TLS, with Windows' own TLS and certificate store).
//! Folders are listed with `MLSD` where the server has it, else `LIST`.

use super::{RemoteConnection, RemoteEntry, RemoteFs, RemoteKind};
use std::io::{Read, Write};
use std::time::UNIX_EPOCH;
use suppaftp::list::File;
use suppaftp::types::FileType;
use suppaftp::{NativeTlsConnector, NativeTlsFtpStream};

pub struct FtpFs {
    ftp: NativeTlsFtpStream,
    /// `false` once the server turned `MLSD` down.
    mlsd: bool,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl FtpFs {
    pub fn connect(conn: &RemoteConnection, secret: &str) -> Result<Self, String> {
        let stream = super::sftp::tcp(&conn.host, conn.port)?;
        let mut ftp =
            NativeTlsFtpStream::connect_with_stream(stream).map_err(|e| format!("FTP: {e}"))?;
        if conn.kind == RemoteKind::Ftps {
            let tls = suppaftp::native_tls::TlsConnector::new().map_err(err)?;
            ftp = ftp
                .into_secure(NativeTlsConnector::from(tls), &conn.host)
                .map_err(|e| format!("Couldn't start TLS: {e}"))?;
        }
        let user = if conn.username.is_empty() {
            "anonymous"
        } else {
            conn.username.as_str()
        };
        ftp.login(user, secret)
            .map_err(|e| format!("Signing in failed: {e}"))?;
        ftp.transfer_type(FileType::Binary).map_err(err)?;
        Ok(Self { ftp, mlsd: true })
    }
}

fn entry(file: File) -> Option<RemoteEntry> {
    let name = file.name().to_string();
    if name == "." || name == ".." || name.is_empty() {
        return None;
    }
    let is_dir = file.is_directory();
    Some(RemoteEntry {
        name,
        is_dir,
        size: (!is_dir).then_some(file.size() as u64),
        modified: file
            .modified()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs() as i64),
    })
}

impl RemoteFs for FtpFs {
    fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>, String> {
        if self.mlsd {
            match self.ftp.mlsd(Some(dir)) {
                Ok(lines) => {
                    return Ok(lines
                        .iter()
                        .filter(|l| {
                            // `type=cdir`/`pdir` are the folder itself and its parent.
                            let lower = l.to_lowercase();
                            !lower.contains("type=cdir") && !lower.contains("type=pdir")
                        })
                        .filter_map(|l| suppaftp::list::ListParser::parse_mlsd(l).ok())
                        .filter_map(entry)
                        .collect());
                }
                Err(_) => self.mlsd = false,
            }
        }
        let lines = self.ftp.list(Some(dir)).map_err(err)?;
        Ok(lines
            .iter()
            .filter_map(|l| File::try_from(l.as_str()).ok())
            .filter_map(entry)
            .collect())
    }

    fn download(
        &mut self,
        file: &str,
        to: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), String> {
        let mut stream = self.ftp.retr_as_stream(file).map_err(err)?;
        super::sftp::copy(&mut stream, to, progress)?;
        stream.finish().map_err(err)
    }

    fn upload(
        &mut self,
        from: &mut dyn Read,
        _size: u64,
        file: &str,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), String> {
        let mut stream = self.ftp.put_with_stream(file).map_err(err)?;
        super::sftp::copy(from, &mut stream, progress)?;
        stream.finish().map_err(err)
    }

    fn mkdir(&mut self, dir: &str) -> Result<(), String> {
        self.ftp.mkdir(dir).map_err(err)
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        self.ftp.rename(from, to).map_err(err)
    }

    fn remove_file(&mut self, file: &str) -> Result<(), String> {
        self.ftp.rm(file).map_err(err)
    }

    fn remove_dir(&mut self, dir: &str) -> Result<(), String> {
        self.ftp.rmdir(dir).map_err(err)
    }
}
