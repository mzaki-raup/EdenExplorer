//! WebDAV (Nextcloud/ownCloud, Synology, IIS, Apache, ...) over HTTP or
//! HTTPS with Windows' own TLS. Folders are listed with `PROPFIND`.

use super::{RemoteConnection, RemoteEntry, RemoteFs};
use std::io::{Read, Write};
use ureq::http::Request;

pub struct WebDavFs {
    agent: ureq::Agent,
    /// `https://host:port/base/path` (no trailing slash).
    base: String,
    /// The base path alone, decoded, for recognizing hrefs.
    base_path: String,
    auth: Option<String>,
}

/// An HTTP client using Windows' TLS, reporting HTTP errors as statuses.
pub(super) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                .build(),
        )
        .http_status_as_error(false)
        // WebDAV's PROPFIND, MKCOL and MOVE.
        .allow_non_standard_methods(true)
        .timeout_connect(Some(std::time::Duration::from_secs(15)))
        .build()
        .into()
}

/// Percent-encodes a path (keeping `/`).
pub(super) fn encode_path(path: &str) -> String {
    let mut out = String::new();
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub(super) fn decode_path(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(b) = u8::from_str_radix(hex, 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(super) fn status_error(status: u16, what: &str) -> String {
    match status {
        401 => "Signing in failed (wrong user name or password).".into(),
        403 => format!("Access denied ({what})."),
        404 => format!("Not found ({what})."),
        _ => format!("The server answered {status} ({what})."),
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl WebDavFs {
    pub fn new(conn: &RemoteConnection, secret: &str) -> Result<Self, String> {
        if conn.host.trim().is_empty() {
            return Err("No server name".into());
        }
        let scheme = if conn.https { "https" } else { "http" };
        let base_path = format!("/{}", super::segments_of(&conn.path).join("/"));
        let base_path = base_path.trim_end_matches('/').to_string();
        let base = format!(
            "{scheme}://{}:{}{}",
            conn.host.trim(),
            conn.port,
            encode_path(&base_path)
        );
        let auth = (!conn.username.is_empty()).then(|| {
            format!(
                "Basic {}",
                base64_padded(format!("{}:{secret}", conn.username).as_bytes())
            )
        });
        let mut fs = Self {
            agent: agent(),
            base,
            base_path,
            auth,
        };
        // Check the server and the sign-in now, so errors show up front.
        fs.list("/")?;
        Ok(fs)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, encode_path(path))
    }

    fn request(&self, method: &str, path: &str) -> ureq::http::request::Builder {
        let mut builder = Request::builder().method(method).uri(self.url(path));
        if let Some(auth) = &self.auth {
            builder = builder.header("Authorization", auth);
        }
        builder
    }

    fn simple(
        &mut self,
        method: &str,
        path: &str,
        extra: &[(&str, String)],
        what: &str,
    ) -> Result<(), String> {
        let mut builder = self.request(method, path);
        for (k, v) in extra {
            builder = builder.header(*k, v);
        }
        let response = self
            .agent
            .run(builder.body(()).map_err(err)?)
            .map_err(err)?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(status_error(status, what))
        }
    }
}

/// Base64 with `=` padding (HTTP Basic auth).
pub(super) fn base64_padded(bytes: &[u8]) -> String {
    let mut s = super::sftp::base64(bytes);
    while !s.len().is_multiple_of(4) {
        s.push('=');
    }
    s
}

/// The items of a `PROPFIND` (Depth: 1) reply: (decoded href path, entry).
fn parse_multistatus(xml: &str) -> Vec<(String, RemoteEntry)> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut out = Vec::new();
    let mut href = String::new();
    let mut is_dir = false;
    let mut size = None;
    let mut modified = None;
    let mut current = String::new();
    let mut in_response = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let local = String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase();
                if local == "response" {
                    in_response = true;
                    href.clear();
                    is_dir = false;
                    size = None;
                    modified = None;
                }
                if local == "collection" {
                    is_dir = true;
                }
                current = local;
            }
            Ok(Event::Empty(e)) => {
                if e.local_name().as_ref().eq_ignore_ascii_case(b"collection") {
                    is_dir = true;
                }
            }
            Ok(Event::Text(t)) if in_response => {
                let text = t.decode().map(|c| c.into_owned()).unwrap_or_default();
                match current.as_str() {
                    "href" => href.push_str(text.trim()),
                    "getcontentlength" => size = text.trim().parse().ok(),
                    "getlastmodified" => modified = super::parse_date(&text),
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                if e.local_name().as_ref().eq_ignore_ascii_case(b"response") {
                    in_response = false;
                    // An href may be a full URL or just the path.
                    let path = match href.find("://") {
                        Some(i) => href[i + 3..]
                            .find('/')
                            .map(|j| &href[i + 3 + j..])
                            .unwrap_or("/")
                            .to_string(),
                        None => href.clone(),
                    };
                    let path = decode_path(&path);
                    let name = path
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or("")
                        .to_string();
                    out.push((
                        path.clone(),
                        RemoteEntry {
                            name,
                            is_dir,
                            size: (!is_dir).then_some(size).flatten(),
                            modified,
                        },
                    ));
                }
                current.clear();
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

impl RemoteFs for WebDavFs {
    fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>, String> {
        const BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getcontentlength/><d:getlastmodified/></d:prop></d:propfind>"#;
        let dir_path = if dir.ends_with('/') {
            dir.to_string()
        } else {
            format!("{dir}/")
        };
        let request = self
            .request("PROPFIND", &dir_path)
            .header("Depth", "1")
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(BODY)
            .map_err(err)?;
        let response = self.agent.run(request).map_err(err)?;
        let status = response.status().as_u16();
        if status != 207 && !(200..300).contains(&status) {
            return Err(status_error(status, "listing the folder"));
        }
        let xml = response.into_body().read_to_string().map_err(err)?;
        let own = format!("{}{}", self.base_path, dir_path.trim_end_matches('/'));
        Ok(parse_multistatus(&xml)
            .into_iter()
            .filter(|(path, entry)| {
                path.trim_end_matches('/') != own.trim_end_matches('/') && !entry.name.is_empty()
            })
            .map(|(_, entry)| entry)
            .collect())
    }

    fn download(
        &mut self,
        file: &str,
        to: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), String> {
        let request = self.request("GET", file).body(()).map_err(err)?;
        let response = self.agent.run(request).map_err(err)?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(status_error(status, "downloading"));
        }
        let mut reader = response.into_body().into_reader();
        super::sftp::copy(&mut reader, to, progress)
    }

    fn upload(
        &mut self,
        from: &mut dyn Read,
        size: u64,
        file: &str,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), String> {
        let mut counting = super::s3::Counting {
            inner: from,
            done: 0,
            progress,
        };
        let request = self
            .request("PUT", file)
            .header("Content-Length", size.to_string())
            .body(ureq::SendBody::from_reader(&mut counting))
            .map_err(err)?;
        let response = self.agent.run(request).map_err(err)?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(status_error(status, "uploading"))
        }
    }

    fn mkdir(&mut self, dir: &str) -> Result<(), String> {
        self.simple("MKCOL", dir, &[], "creating the folder")
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        let destination = self.url(to);
        self.simple(
            "MOVE",
            from,
            &[("Destination", destination), ("Overwrite", "F".into())],
            "renaming",
        )
    }

    fn remove_file(&mut self, file: &str) -> Result<(), String> {
        self.simple("DELETE", file, &[], "deleting")
    }

    fn remove_dir(&mut self, dir: &str) -> Result<(), String> {
        let dir = if dir.ends_with('/') {
            dir.to_string()
        } else {
            format!("{dir}/")
        };
        self.simple("DELETE", &dir, &[], "deleting")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_propfind_reply() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
 <d:response><d:href>/dav/docs/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
 <d:response><d:href>http://srv/dav/docs/a%20b.txt</d:href><d:propstat><d:prop><d:resourcetype/><d:getcontentlength>12</d:getcontentlength><d:getlastmodified>Tue, 15 Nov 1994 08:12:31 GMT</d:getlastmodified></d:prop></d:propstat></d:response>
 <d:response><d:href>/dav/docs/sub/</d:href><d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat></d:response>
</d:multistatus>"#;
        let items = parse_multistatus(xml);
        assert_eq!(items.len(), 3);
        assert_eq!(items[1].0, "/dav/docs/a b.txt");
        assert_eq!(
            items[1].1,
            RemoteEntry {
                name: "a b.txt".into(),
                is_dir: false,
                size: Some(12),
                modified: Some(784887151)
            }
        );
        assert!(items[2].1.is_dir && items[2].1.name == "sub");
    }

    #[test]
    fn encodes_paths() {
        assert_eq!(encode_path("/a b/ü.txt"), "/a%20b/%C3%BC.txt");
        assert_eq!(decode_path("/a%20b/%C3%BC.txt"), "/a b/ü.txt");
        assert_eq!(base64_padded(b"user:pass"), "dXNlcjpwYXNz");
        assert_eq!(base64_padded(b"a"), "YQ==");
    }
}
