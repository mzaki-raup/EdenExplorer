//! Apache Solr, browsed like a folder tree. **Read-only.**
//!
//! ```text
//! /                    every core     GET /solr/admin/cores?action=STATUS
//! /articles            its documents  GET /solr/articles/select?cursorMark=...
//! /articles/7.json     one document   GET /solr/articles/get?id=7
//! ```
//!
//! Four things worth knowing:
//!
//! * **Listing walks `cursorMark`, not `start`/`rows`.** Deep offset paging
//!   in Solr makes every shard build and discard the skipped documents, so
//!   it gets slower the further in it goes; a cursor is constant cost. Solr's
//!   own rule for "the walk is finished" is that the returned
//!   `nextCursorMark` equals the one just sent - not an empty page - and that
//!   is what this checks, because the last page comes back *full* with the
//!   same mark.
//! * **A cursor requires a total ordering**, so every request sends
//!   `sort=id asc`. Without it Solr refuses the cursor outright rather than
//!   quietly paging wrong.
//! * **Solr does not always answer in JSON.** A path that doesn't name a core
//!   returns an HTML error page, so a failure has to survive a body that
//!   isn't JSON at all - the shared layer falls back to the status for that.
//! * **The Modified column is left blank.** Documents carry a `_version_`
//!   whose high bits happen to be a millisecond clock, which is tempting -
//!   but that encoding is Solr's internal version stamp, not a documented
//!   field, and a date silently derived from an implementation detail is
//!   worse than an honest blank. `elasticsearch` fills the column only
//!   because `@timestamp` is a real convention; there is no equivalent here.
//!
//! Nothing here writes, for the same reason as every other store in this
//! directory.

use super::http_json::{Auth, JsonHttp, file_name_for, id_from_file_name, json_size, pretty};
use super::webdav::encode_path;
use super::{RemoteConnection, RemoteEntry, RemoteFs};
use serde_json::Value;
use std::io::{Read, Write};

/// Documents asked for in one cursor page.
const PAGE: usize = 1_000;

/// Stop listing a core after this many documents.
const MAX_DOCS: usize = 20_000;

/// Where Solr lives under the host. Its own default, and what every
/// distribution ships.
const CONTEXT: &str = "/solr";

pub struct SolrFs {
    http: JsonHttp,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn read_only() -> String {
    "This Solr location is read-only.".to_string()
}

/// Solr explains itself in `error.msg` when it answers in JSON at all.
fn detail(value: &Value) -> Option<String> {
    Some(value.get("error")?.get("msg")?.as_str()?.to_string())
}

/// `/articles/7.json` → the core and the document it names.
fn split(path: &str) -> (Option<String>, Option<String>) {
    let mut parts = super::segments_of(path).into_iter();
    (
        parts.next(),
        parts.next().as_deref().map(id_from_file_name),
    )
}

/// A document's `id`, as a string however it is typed.
fn id_text(value: &Value) -> Option<String> {
    match value.get("id")? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Percent-encodes a cursor mark for a query string.
///
/// A mark is base64 and so can contain `+`, `/` and `=`, every one of which
/// means something else in a query string - `+` in particular would arrive as
/// a space and Solr would reject the cursor.
fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

impl SolrFs {
    pub fn new(conn: &RemoteConnection, secret: &str) -> Result<Self, String> {
        let fs = SolrFs {
            http: JsonHttp::new(
                &conn.host,
                conn.port,
                conn.https,
                // Solr's security plugin uses HTTP Basic; an unsecured
                // instance (the default) sends nothing, which `Auth` handles
                // by having no credential to send.
                Auth::Basic {
                    user: conn.username.clone(),
                    secret: secret.to_string(),
                },
            )?
            .with_detail(detail),
        };

        // Names the product, so a server that answers but isn't Solr is
        // reported as such rather than failing later with something cryptic.
        let system = fs
            .http
            .get_json(&format!("{CONTEXT}/admin/info/system?wt=json"), "the server")?;
        if system.get("lucene").is_none() && system.get("solr_home").is_none() {
            return Err("That server answered, but it isn't Solr.".into());
        }
        Ok(fs)
    }

    /// Every core, from the admin API.
    fn cores(&self) -> Result<Vec<String>, String> {
        let body = self.http.get_json(
            &format!("{CONTEXT}/admin/cores?action=STATUS&wt=json"),
            "the core list",
        )?;
        let mut names: Vec<String> = body
            .get("status")
            .and_then(Value::as_object)
            .map(|status| status.keys().cloned().collect())
            .unwrap_or_default();
        names.sort();
        Ok(names)
    }

    /// Every document in a core, up to `MAX_DOCS`, following `cursorMark`.
    fn documents(&self, core: &str) -> Result<Vec<Value>, String> {
        let core = encode_path(core);
        let mut out: Vec<Value> = Vec::new();
        let mut cursor = "*".to_string();

        while out.len() < MAX_DOCS {
            let page = self.http.get_json(
                &format!(
                    "{CONTEXT}/{core}/select?q=*%3A*&rows={PAGE}&sort=id+asc\
                     &cursorMark={}&wt=json",
                    encode_query(&cursor)
                ),
                "the documents",
            )?;
            let batch: Vec<Value> = page
                .get("response")
                .and_then(|response| response.get("docs"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            out.extend(batch);

            // Solr's own rule for a finished walk: the mark it returns is the
            // one you sent. The final page arrives *full*, so stopping on an
            // empty batch would cost an extra round trip - and a cursor that
            // never compares marks would loop for ever on that last page.
            let next = page
                .get("nextCursorMark")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if next.is_empty() || next == cursor {
                break;
            }
            cursor = next;
        }

        out.truncate(MAX_DOCS);
        Ok(out)
    }
}

impl RemoteFs for SolrFs {
    fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>, String> {
        match split(dir) {
            (None, _) => Ok(self
                .cores()?
                .into_iter()
                .map(|name| RemoteEntry {
                    name,
                    is_dir: true,
                    size: None,
                    modified: None,
                })
                .collect()),
            (Some(core), None) => {
                let documents = self.documents(&core)?;
                Ok(documents
                    .iter()
                    .filter_map(|document| {
                        Some(RemoteEntry {
                            name: file_name_for(&id_text(document)?),
                            is_dir: false,
                            size: Some(json_size(document)),
                            // See the module docs: `_version_` is not a date.
                            modified: None,
                        })
                    })
                    .collect())
            }
            // A document is not a folder.
            _ => Err("Not a folder.".into()),
        }
    }

    fn download(
        &mut self,
        file: &str,
        to: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), String> {
        let (Some(core), Some(id)) = split(file) else {
            return Err("Not a document.".into());
        };
        let body = self.http.get_json(
            &format!(
                "{CONTEXT}/{}/get?id={}&wt=json",
                encode_path(&core),
                encode_query(&id)
            ),
            "the document",
        )?;
        // `/get` wraps the document in `{"doc": ...}`, and answers
        // `{"doc": null}` for one that isn't there - which is a missing
        // document, not an empty one.
        let document = match body.get("doc") {
            Some(Value::Null) | None => {
                return Err(format!("There is no document called {id} in {core}."));
            }
            Some(document) => document,
        };
        let text = pretty(document);
        to.write_all(text.as_bytes()).map_err(err)?;
        progress(text.len() as u64);
        Ok(())
    }

    fn upload(
        &mut self,
        _from: &mut dyn Read,
        _size: u64,
        _file: &str,
        _progress: &mut dyn FnMut(u64),
    ) -> Result<(), String> {
        Err(read_only())
    }

    fn mkdir(&mut self, _dir: &str) -> Result<(), String> {
        Err(read_only())
    }

    fn rename(&mut self, _from: &str, _to: &str) -> Result<(), String> {
        Err(read_only())
    }

    fn remove_file(&mut self, _file: &str) -> Result<(), String> {
        Err(read_only())
    }

    fn remove_dir(&mut self, _dir: &str) -> Result<(), String> {
        Err(read_only())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::remote::RemoteKind;

    #[test]
    fn a_path_splits_into_a_core_and_a_document() {
        assert_eq!(split("/"), (None, None));
        assert_eq!(split("/articles"), (Some("articles".into()), None));
        assert_eq!(
            split("/articles/7.json"),
            (Some("articles".into()), Some("7".into()))
        );
    }

    /// A cursor mark is base64, so it can carry `+`, `/` and `=` - every one
    /// of which means something else in a query string. `+` arriving as a
    /// space is what breaks a cursor walk silently.
    #[test]
    fn a_cursor_mark_is_escaped_for_a_query_string() {
        assert_eq!(encode_query("AoEiMTA="), "AoEiMTA%3D");
        assert_eq!(encode_query("a+b/c="), "a%2Bb%2Fc%3D");
        assert_eq!(encode_query("plain-id_1.0~"), "plain-id_1.0~");
    }

    #[test]
    fn the_servers_own_message_is_what_a_failure_reports() {
        let body = serde_json::json!({"error": {"msg": "no such core: nope", "code": 404}});
        assert_eq!(detail(&body).as_deref(), Some("no such core: nope"));
        assert_eq!(detail(&serde_json::json!({"response": {}})), None);
    }

    /// A stand-in Solr speaking just enough of `select`/`get` to drive a real
    /// `SolrFs` over a real socket.
    mod fake {
        use std::io::{BufRead, BufReader, Read as _, Write};
        use std::net::{TcpListener, TcpStream};
        use std::sync::{Arc, Mutex};

        pub struct Seen {
            pub calls: Vec<String>,
            pub auth: Option<String>,
        }

        /// `total` documents, handed out `batch` at a time. The last page
        /// comes back **full** with the same cursor mark, exactly as Solr
        /// does - which is what makes "stop on an empty page" wrong.
        pub fn start(total: usize, batch: usize) -> (u16, Arc<Mutex<Seen>>) {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().expect("addr").port();
            let seen = Arc::new(Mutex::new(Seen {
                calls: Vec::new(),
                auth: None,
            }));
            let recorder = Arc::clone(&seen);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { break };
                    let recorder = Arc::clone(&recorder);
                    std::thread::spawn(move || serve(stream, total, batch, recorder));
                }
            });
            (port, seen)
        }

        /// The mark is just the next index, as `c<N>`; `*` means the start.
        fn mark_index(path: &str) -> usize {
            let raw = path
                .split(['?', '&'])
                .filter_map(|part| part.strip_prefix("cursorMark="))
                .next()
                .unwrap_or("*");
            if raw == "*" || raw == "%2A" {
                return 0;
            }
            raw.trim_start_matches('c').parse().unwrap_or(0)
        }

        fn serve(stream: TcpStream, total: usize, batch: usize, seen: Arc<Mutex<Seen>>) {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut stream = stream;
            loop {
                let mut request_line = String::new();
                if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                    return;
                }
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let path = parts.next().unwrap_or("").to_string();

                let mut auth = None;
                let mut content_length = 0usize;
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 {
                        return;
                    }
                    let header = header.trim_end().to_string();
                    if header.is_empty() {
                        break;
                    }
                    let lower = header.to_ascii_lowercase();
                    if let Some(value) = lower.strip_prefix("authorization:") {
                        auth = Some(header[header.len() - value.trim().len()..].to_string());
                    }
                    if let Some(value) = lower.strip_prefix("content-length:") {
                        content_length = value.trim().parse().unwrap_or(0);
                    }
                }
                if content_length > 0 {
                    let mut raw = vec![0u8; content_length];
                    let _ = reader.read_exact(&mut raw);
                }

                {
                    let mut seen = seen.lock().expect("lock");
                    seen.calls.push(format!("{method} {path}"));
                    if auth.is_some() {
                        seen.auth = auth.clone();
                    }
                }

                let body = if path.contains("/admin/info/system") {
                    r#"{"lucene":{"solr-spec-version":"9.11.0"},"solr_home":"/var/solr/data"}"#
                        .to_string()
                } else if path.contains("/admin/cores") {
                    r#"{"status":{"articles":{"name":"articles"},"notes":{"name":"notes"}}}"#
                        .to_string()
                } else if path.contains("/select") {
                    let start = mark_index(&path);
                    let end = (start + batch).min(total);
                    let docs: Vec<String> = (start..end)
                        .map(|i| format!(r#"{{"id":"{i}","title_s":"Article {i}"}}"#))
                        .collect();
                    // Solr returns the *same* mark once the walk is done,
                    // and the final page is full rather than empty.
                    let next = if end >= total {
                        path.split(['?', '&'])
                            .filter_map(|p| p.strip_prefix("cursorMark="))
                            .next()
                            .unwrap_or("*")
                            .to_string()
                    } else {
                        format!("c{end}")
                    };
                    format!(
                        r#"{{"response":{{"numFound":{total},"docs":[{}]}},"nextCursorMark":"{next}"}}"#,
                        docs.join(",")
                    )
                } else if path.contains("/get?") {
                    let id = path
                        .split(['?', '&'])
                        .filter_map(|p| p.strip_prefix("id="))
                        .next()
                        .unwrap_or("");
                    if id == "missing" {
                        r#"{"doc":null}"#.to_string()
                    } else {
                        format!(r#"{{"doc":{{"id":"{id}","title_s":"Article {id}"}}}}"#)
                    }
                } else {
                    // Solr really does answer some paths with HTML, which is
                    // what the shared layer's non-JSON fallback is for.
                    let response = "HTTP/1.1 404 Not Found\r\nContent-Type: text/html\r\n\
                         Content-Length: 22\r\nConnection: keep-alive\r\n\r\n\
                         <html>Not found</html>";
                    if stream.write_all(response.as_bytes()).is_err() {
                        return;
                    }
                    continue;
                };

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                     Connection: keep-alive\r\n\r\n{body}",
                    body.len()
                );
                if stream.write_all(response.as_bytes()).is_err() {
                    return;
                }
            }
        }
    }

    fn conn_for(port: u16) -> RemoteConnection {
        RemoteConnection {
            id: 900_012,
            kind: RemoteKind::Solr,
            host: "127.0.0.1".into(),
            port,
            https: false,
            ..Default::default()
        }
    }

    #[test]
    fn cores_list_as_folders() {
        let (port, _seen) = fake::start(1, PAGE);
        let mut fs = SolrFs::new(&conn_for(port), "").expect("connect");
        let top = fs.list("/").expect("list the root");
        let names: Vec<&str> = top.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["articles", "notes"]);
        assert!(top.iter().all(|e| e.is_dir));
    }

    /// The cursor has to terminate on a *repeated mark*, not an empty page -
    /// Solr's last page comes back full, so a walk that waits for an empty
    /// one asks for the same page for ever.
    #[test]
    fn documents_are_walked_by_cursor_and_the_walk_terminates() {
        let (port, seen) = fake::start(2_500, PAGE);
        let mut fs = SolrFs::new(&conn_for(port), "").expect("connect");
        let docs = fs.list("/articles").expect("list documents");
        assert_eq!(docs.len(), 2_500);

        let mut names: Vec<String> = docs.iter().map(|e| e.name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 2_500, "a document was listed twice");

        let seen = seen.lock().expect("lock");
        let selects = seen.calls.iter().filter(|c| c.contains("/select")).count();
        assert_eq!(selects, 3, "2500 documents in pages of 1000");
        // A cursor needs a total ordering or Solr refuses it outright.
        assert!(
            seen.calls
                .iter()
                .filter(|c| c.contains("/select"))
                .all(|c| c.contains("sort=id+asc")),
            "a select went without a sort: {:?}",
            seen.calls
        );
    }

    /// An exactly-full last page is the case that loops for ever if the walk
    /// stops on an empty batch rather than a repeated mark.
    #[test]
    fn a_core_whose_size_is_an_exact_multiple_of_the_page_still_terminates() {
        let (port, seen) = fake::start(2 * PAGE, PAGE);
        let mut fs = SolrFs::new(&conn_for(port), "").expect("connect");
        let docs = fs.list("/articles").expect("list documents");
        assert_eq!(docs.len(), 2 * PAGE);
        let seen = seen.lock().expect("lock");
        assert_eq!(
            seen.calls.iter().filter(|c| c.contains("/select")).count(),
            2,
            "an exactly-full last page should not cost an extra request"
        );
    }

    #[test]
    fn a_document_opens_pretty_printed_without_its_envelope() {
        let (port, _seen) = fake::start(10, PAGE);
        let mut fs = SolrFs::new(&conn_for(port), "").expect("connect");
        let mut out = Vec::new();
        fs.download("/articles/7.json", &mut out, &mut |_| {})
            .expect("download");
        let text = String::from_utf8(out).expect("utf-8");
        assert!(text.contains("\"title_s\": \"Article 7\""), "got {text}");
        assert!(!text.contains("\"doc\""), "the envelope leaked: {text}");
    }

    /// `{"doc": null}` means the document isn't there - not that it is empty.
    #[test]
    fn a_missing_document_is_an_error_rather_than_an_empty_file() {
        let (port, _seen) = fake::start(10, PAGE);
        let mut fs = SolrFs::new(&conn_for(port), "").expect("connect");
        let mut out = Vec::new();
        let error = match fs.download("/articles/missing.json", &mut out, &mut |_| {}) {
            Ok(()) => panic!("a missing document was reported as an empty one"),
            Err(error) => error,
        };
        assert!(error.contains("no document"), "got {error}");
        assert!(out.is_empty());
    }

    #[test]
    fn basic_auth_is_sent_when_there_is_a_credential() {
        let (port, seen) = fake::start(1, PAGE);
        let mut conn = conn_for(port);
        conn.username = "solr".into();
        let mut fs = SolrFs::new(&conn, "SolrRocks").expect("connect");
        let _ = fs.list("/");
        let seen = seen.lock().expect("lock");
        // base64("solr:SolrRocks")
        assert_eq!(seen.auth.as_deref(), Some("Basic c29scjpTb2xyUm9ja3M="));
    }

    /// An unsecured Solr - the default - must not be sent an empty
    /// credential, which some setups reject outright.
    #[test]
    fn no_credential_means_no_authorization_header() {
        let (port, seen) = fake::start(1, PAGE);
        let mut fs = SolrFs::new(&conn_for(port), "").expect("connect");
        let _ = fs.list("/");
        let seen = seen.lock().expect("lock");
        assert_eq!(seen.auth, None);
    }

    #[test]
    fn every_write_is_refused() {
        let (port, _seen) = fake::start(1, PAGE);
        let mut fs = SolrFs::new(&conn_for(port), "").expect("connect");
        assert!(fs.mkdir("/new").is_err());
        assert!(fs.rename("/articles/1.json", "/articles/2.json").is_err());
        assert!(fs.remove_file("/articles/1.json").is_err());
        assert!(fs.remove_dir("/articles").is_err());
        assert!(
            fs.upload(&mut std::io::empty(), 0, "/articles/1.json", &mut |_| {})
                .is_err()
        );
    }

    /// Against a **real** Solr, not the stand-in above.
    ///
    /// `#[ignore]`d because it needs one running:
    ///
    /// ```text
    /// EDEN_TEST_SOLR=127.0.0.1:8983 cargo test -- --ignored real_solr
    /// ```
    ///
    /// Verified against Solr 9.11.0.
    #[test]
    #[ignore]
    fn a_real_solr_walks_its_cores_by_cursor() {
        let Ok(endpoint) = std::env::var("EDEN_TEST_SOLR") else {
            panic!("set EDEN_TEST_SOLR=host:port");
        };
        let (host, port) = endpoint.rsplit_once(':').expect("host:port");
        let mut conn = RemoteConnection {
            id: 900_012,
            kind: RemoteKind::Solr,
            host: host.to_string(),
            port: port.parse().expect("a port"),
            https: false,
            ..Default::default()
        };
        conn.username = std::env::var("EDEN_TEST_SOLR_USER").unwrap_or_default();
        let secret = std::env::var("EDEN_TEST_SOLR_PASSWORD").unwrap_or_default();

        let mut fs = SolrFs::new(&conn, &secret).expect("connect");
        let top = fs.list("/").expect("list the cores");
        assert!(!top.is_empty(), "no cores");

        let names: Vec<String> = top.iter().map(|e| e.name.clone()).collect();
        let mut biggest = 0usize;
        for core in &names {
            let docs = fs.list(&format!("/{core}")).expect("list documents");
            assert!(!docs.is_empty(), "{core} looked empty");
            biggest = biggest.max(docs.len());

            let mut seen: Vec<String> = docs.iter().map(|e| e.name.clone()).collect();
            seen.sort();
            let before = seen.len();
            seen.dedup();
            assert_eq!(seen.len(), before, "{core} listed a document twice");

            let mut out = Vec::new();
            fs.download(&format!("/{core}/{}", docs[0].name), &mut out, &mut |_| {})
                .expect("download");
            assert!(!out.is_empty(), "a document in {core} came back empty");
        }
        assert!(
            biggest > PAGE,
            "seed a core with more than {PAGE} documents or this proves \
             nothing about the cursor - the biggest here had {biggest}"
        );

        assert!(fs.remove_file("/articles/1.json").is_err(), "read-only");
    }
}
