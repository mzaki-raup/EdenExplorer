//! Meilisearch, browsed like a folder tree. **Read-only.**
//!
//! ```text
//! /                       every index      GET /indexes
//! /movies                 its documents    GET /indexes/movies/documents
//! /movies/7.json          one document     GET /indexes/movies/documents/7
//! ```
//!
//! Three things worth knowing:
//!
//! * **An index's documents are keyed by that index's own `primaryKey`**,
//!   which varies per index (`id` in one, `isbn` in the next) and is reported
//!   by `GET /indexes`. Listing therefore reads the index list first and
//!   keeps the key, rather than guessing at a field called `id` - an index
//!   keyed by something else would otherwise list as a folder full of
//!   documents with no names.
//! * **Paging is `offset`/`limit`, because Meilisearch offers nothing else**
//!   for walking documents - there is no cursor here the way `elasticsearch`
//!   has a scroll and `qdrant` has a scroll offset. The response carries
//!   `total`, so the walk stops as soon as it has seen everything rather
//!   than probing for an empty page.
//! * **An index folder has a real Modified time** (`updatedAt` from the index
//!   list); individual documents don't, because Meilisearch stores no
//!   per-document timestamp and inventing one would be worse than a blank
//!   column.
//!
//! Nothing here writes - same reasoning as `redis`, `couchdb`, `mongodb` and
//! `elasticsearch`: a file manager's reflexes are fast and destructive, and a
//! live search index has no Recycle Bin.

use super::http_json::{Auth, JsonHttp, file_name_for, id_from_file_name, json_size, pretty};
use super::webdav::encode_path;
use super::{RemoteConnection, RemoteEntry, RemoteFs};
use serde_json::Value;
use std::io::{Read, Write};

/// Documents asked for in one page.
const PAGE: usize = 1_000;

/// Stop listing an index after this many documents.
const MAX_DOCS: usize = 20_000;

/// Indexes asked for in one page of the index list.
const INDEX_PAGE: usize = 1_000;

pub struct MeiliFs {
    http: JsonHttp,
}

/// One index, as the index list describes it.
struct Index {
    uid: String,
    /// The field naming each document. Absent until an index has had its
    /// first document, at which point Meilisearch infers one.
    primary_key: Option<String>,
    /// `updatedAt`, which is the closest thing an index has to a modified
    /// time.
    modified: Option<i64>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn read_only() -> String {
    "This Meilisearch location is read-only.".to_string()
}

/// Meilisearch explains itself in `message`, which is consistently more use
/// than the status ("Index `nope` not found." beats "404").
fn detail(value: &Value) -> Option<String> {
    Some(value.get("message")?.as_str()?.to_string())
}

/// `/movies/7.json` → the index and the document it names.
fn split(path: &str) -> (Option<String>, Option<String>) {
    let mut parts = super::segments_of(path).into_iter();
    (
        parts.next(),
        parts.next().as_deref().map(id_from_file_name),
    )
}

/// RFC 3339 (what Meilisearch's `updatedAt` is) as seconds since the epoch.
fn timestamp_of(value: &Value) -> Option<i64> {
    let text = value.get("updatedAt")?.as_str()?;
    Some(
        chrono::DateTime::parse_from_rfc3339(text)
            .ok()?
            .timestamp(),
    )
}

/// A document's own id, as a string, however it is typed.
///
/// Meilisearch allows an integer or a string primary key, and `7` and `"7"`
/// name the same document - so a number is rendered without JSON quoting
/// rather than passed through `to_string`.
fn id_of(document: &Value, primary_key: &str) -> Option<String> {
    match document.get(primary_key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

impl MeiliFs {
    pub fn new(conn: &RemoteConnection, secret: &str) -> Result<Self, String> {
        let http = JsonHttp::new(
            &conn.host,
            conn.port,
            conn.https,
            // Meilisearch takes a bearer token (the master key, or an API
            // key), not a username - so the connection's user field plays no
            // part here.
            Auth::Bearer(secret.to_string()),
        )?
        .with_detail(detail);
        let fs = MeiliFs { http };

        // Two checks, failing for reasons a person can act on separately:
        // `/health` says something is listening and really is Meilisearch,
        // and the index list says the key was accepted.
        let health = fs.http.get_json("/health", "the server")?;
        if health.get("status").and_then(Value::as_str) != Some("available") {
            return Err("That server answered, but it isn't Meilisearch.".into());
        }
        fs.indexes()?;
        Ok(fs)
    }

    /// Every index, with the primary key each one uses and when it last
    /// changed.
    fn indexes(&self) -> Result<Vec<Index>, String> {
        let page = self
            .http
            .get_json(&format!("/indexes?limit={INDEX_PAGE}"), "the index list")?;
        let mut out: Vec<Index> = page
            .get("results")
            .and_then(Value::as_array)
            .map(|results| {
                results
                    .iter()
                    .filter_map(|index| {
                        Some(Index {
                            uid: index.get("uid")?.as_str()?.to_string(),
                            primary_key: index
                                .get("primaryKey")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            modified: timestamp_of(index),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by(|a, b| a.uid.cmp(&b.uid));
        Ok(out)
    }

    /// The primary key of one index, which is what names its documents.
    fn primary_key_of(&self, index: &str) -> Result<String, String> {
        self.indexes()?
            .into_iter()
            .find(|candidate| candidate.uid == index)
            .map(|candidate| candidate.primary_key)
            .ok_or_else(|| format!("There is no index called {index}."))?
            .ok_or_else(|| {
                format!("The index {index} has no primary key yet, so it has no documents.")
            })
    }

    /// Every document in an index, up to `MAX_DOCS`.
    fn documents(&self, index: &str) -> Result<Vec<Value>, String> {
        let index = encode_path(index);
        let mut out: Vec<Value> = Vec::new();
        let mut offset = 0usize;
        loop {
            let page = self.http.get_json(
                &format!("/indexes/{index}/documents?limit={PAGE}&offset={offset}"),
                "the documents",
            )?;
            let batch = page
                .get("results")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if batch.is_empty() {
                break;
            }
            offset += batch.len();
            out.extend(batch);

            // `total` is reported, so stop on knowing rather than on an
            // empty page - one fewer request per folder.
            let total = page.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
            if out.len() >= total.min(MAX_DOCS) {
                break;
            }
        }
        out.truncate(MAX_DOCS);
        Ok(out)
    }
}

impl RemoteFs for MeiliFs {
    fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>, String> {
        match split(dir) {
            (None, _) => Ok(self
                .indexes()?
                .into_iter()
                .map(|index| RemoteEntry {
                    name: index.uid,
                    is_dir: true,
                    size: None,
                    modified: index.modified,
                })
                .collect()),
            (Some(index), None) => {
                let primary_key = self.primary_key_of(&index)?;
                let documents = self.documents(&index)?;
                Ok(documents
                    .iter()
                    .filter_map(|document| {
                        Some(RemoteEntry {
                            name: file_name_for(&id_of(document, &primary_key)?),
                            is_dir: false,
                            size: Some(json_size(document)),
                            // Meilisearch keeps no per-document timestamp.
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
        let (Some(index), Some(id)) = split(file) else {
            return Err("Not a document.".into());
        };
        let document = self.http.get_json(
            &format!(
                "/indexes/{}/documents/{}",
                encode_path(&index),
                encode_path(&id)
            ),
            "the document",
        )?;
        let text = pretty(&document);
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
    fn a_path_splits_into_an_index_and_a_document() {
        assert_eq!(split("/"), (None, None));
        assert_eq!(split("/movies"), (Some("movies".into()), None));
        assert_eq!(
            split("/movies/7.json"),
            (Some("movies".into()), Some("7".into()))
        );
        // The escaping the shared file-name helper applies is undone here.
        assert_eq!(
            split("/movies/a%2Fb.json"),
            (Some("movies".into()), Some("a/b".into()))
        );
    }

    /// `7` and `"7"` name the same document, so a numeric key must not come
    /// back carrying JSON quotes.
    #[test]
    fn a_numeric_primary_key_is_not_quoted() {
        let doc = serde_json::json!({"id": 7, "title": "Movie 7"});
        assert_eq!(id_of(&doc, "id").as_deref(), Some("7"));
        let doc = serde_json::json!({"isbn": "isbn-3"});
        assert_eq!(id_of(&doc, "isbn").as_deref(), Some("isbn-3"));
        // A key the document doesn't carry yields nothing rather than a name
        // that couldn't be fetched back.
        assert_eq!(id_of(&doc, "id"), None);
    }

    #[test]
    fn an_index_timestamp_is_read_from_updated_at() {
        let index = serde_json::json!({
            "uid": "movies",
            "updatedAt": "2026-10-10T13:20:39.946329925Z"
        });
        // 2026-10-10T13:20:39Z, computed independently of the code under
        // test rather than copied from its output.
        assert_eq!(timestamp_of(&index), Some(1_791_638_439));
        assert_eq!(timestamp_of(&serde_json::json!({"uid": "movies"})), None);
    }

    #[test]
    fn the_servers_own_message_is_what_a_failure_reports() {
        let body = serde_json::json!({
            "message": "Index `nope` not found.",
            "code": "index_not_found"
        });
        assert_eq!(detail(&body).as_deref(), Some("Index `nope` not found."));
        assert_eq!(detail(&serde_json::json!({"code": "x"})), None);
    }

    /// A stand-in Meilisearch speaking just enough of the document API to
    /// drive a real `MeiliFs` over a real socket.
    ///
    /// The same approach as `redis`, `couchdb`, `mongodb` and
    /// `elasticsearch`. What a unit test can't reach is the paging loop, the
    /// bearer header, and the paths the client actually asks for - all of
    /// which this records so a test can assert on them.
    mod fake {
        use std::io::{BufRead, BufReader, Write};
        use std::net::{TcpListener, TcpStream};
        use std::sync::{Arc, Mutex};

        pub struct Seen {
            /// `METHOD path` for every request.
            pub calls: Vec<String>,
            pub auth: Option<String>,
        }

        /// Starts the stand-in. `total` documents live in `movies`, handed
        /// out `batch` at a time so the paging loop has to go round.
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
                    std::thread::spawn(move || {
                        serve(stream, total, batch, recorder);
                    });
                }
            });
            (port, seen)
        }

        fn query_number(path: &str, key: &str) -> usize {
            path.split(['?', '&'])
                .filter_map(|part| part.strip_prefix(&format!("{key}=")))
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0)
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
                    let mut body = vec![0u8; content_length];
                    use std::io::Read as _;
                    let _ = reader.read_exact(&mut body);
                }

                {
                    let mut seen = seen.lock().expect("lock");
                    seen.calls.push(format!("{method} {path}"));
                    if auth.is_some() {
                        seen.auth = auth.clone();
                    }
                }

                let body = if path == "/health" {
                    r#"{"status":"available"}"#.to_string()
                } else if path.starts_with("/indexes?") || path == "/indexes" {
                    r#"{"results":[{"uid":"movies","primaryKey":"id","updatedAt":"2026-10-10T13:20:39Z"},{"uid":"empty","primaryKey":null}],"offset":0,"limit":1000,"total":2}"#.to_string()
                } else if path.starts_with("/indexes/movies/documents/") {
                    let id = path.rsplit('/').next().unwrap_or("");
                    format!(r#"{{"id":{id},"title":"Movie {id}"}}"#)
                } else if path.starts_with("/indexes/movies/documents") {
                    let offset = query_number(&path, "offset");
                    let limit = query_number(&path, "limit").min(batch);
                    let end = (offset + limit).min(total);
                    let docs: Vec<String> = (offset..end)
                        .map(|i| format!(r#"{{"id":{i},"title":"Movie {i}"}}"#))
                        .collect();
                    format!(
                        r#"{{"results":[{}],"offset":{offset},"limit":{limit},"total":{total}}}"#,
                        docs.join(",")
                    )
                } else {
                    let response = "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\n\
                         Content-Length: 46\r\nConnection: keep-alive\r\n\r\n\
                         {\"message\":\"Not found.\",\"code\":\"not_found\"}";
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
            id: 900_010,
            kind: RemoteKind::Meilisearch,
            host: "127.0.0.1".into(),
            port,
            https: false,
            ..Default::default()
        }
    }

    #[test]
    fn indexes_list_as_folders_with_their_own_modified_time() {
        let (port, _seen) = fake::start(0, 10);
        let mut fs = MeiliFs::new(&conn_for(port), "k").expect("connect");
        let top = fs.list("/").expect("list the root");
        let names: Vec<&str> = top.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["empty", "movies"], "sorted, and both present");
        assert!(top.iter().all(|e| e.is_dir));
        let movies = top.iter().find(|e| e.name == "movies").expect("movies");
        assert_eq!(movies.modified, Some(1_791_638_439));
        // The index with no primary key reports no timestamp rather than a
        // made-up one.
        let empty = top.iter().find(|e| e.name == "empty").expect("empty");
        assert_eq!(empty.modified, None);
    }

    /// The bearer header is what Meilisearch documents, and the paging loop
    /// has to go round - both invisible to a unit test.
    #[test]
    fn documents_are_paged_until_the_total_is_reached() {
        let (port, seen) = fake::start(2_500, PAGE);
        let mut fs = MeiliFs::new(&conn_for(port), "eden-test-master-key").expect("connect");
        let docs = fs.list("/movies").expect("list movies");
        assert_eq!(docs.len(), 2_500);

        // Every document appears exactly once - the off-by-one that
        // duplicates or drops one per page.
        let mut names: Vec<String> = docs.iter().map(|e| e.name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 2_500, "a document was listed twice");
        assert!(docs.iter().all(|e| !e.is_dir && e.size.is_some()));

        let seen = seen.lock().expect("lock");
        assert_eq!(
            seen.auth.as_deref(),
            Some("Bearer eden-test-master-key"),
            "Meilisearch takes a bearer token"
        );
        let document_calls = seen
            .calls
            .iter()
            .filter(|c| c.contains("/documents?"))
            .count();
        assert_eq!(document_calls, 3, "2500 documents in pages of 1000");
    }

    #[test]
    fn a_document_opens_pretty_printed() {
        let (port, _seen) = fake::start(10, PAGE);
        let mut fs = MeiliFs::new(&conn_for(port), "k").expect("connect");
        let mut out = Vec::new();
        fs.download("/movies/7.json", &mut out, &mut |_| {})
            .expect("download");
        let text = String::from_utf8(out).expect("utf-8");
        assert!(text.contains("\"title\": \"Movie 7\""), "got {text}");
        assert!(text.contains('\n'), "should be pretty-printed");
    }

    #[test]
    fn every_write_is_refused() {
        let (port, _seen) = fake::start(1, PAGE);
        let mut fs = MeiliFs::new(&conn_for(port), "k").expect("connect");
        assert!(fs.mkdir("/new").is_err());
        assert!(fs.rename("/movies/1.json", "/movies/2.json").is_err());
        assert!(fs.remove_file("/movies/1.json").is_err());
        assert!(fs.remove_dir("/movies").is_err());
        assert!(
            fs.upload(&mut std::io::empty(), 0, "/movies/9.json", &mut |_| {})
                .is_err()
        );
    }

    /// A server that answers but isn't Meilisearch is said so, rather than
    /// failing later with something cryptic.
    #[test]
    fn a_server_that_is_not_meilisearch_is_refused() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                // Drain the request first: closing a socket with the
                // client's bytes still unread makes Windows reset the
                // connection, and the test then fails with an I/O error
                // instead of the refusal it exists to check.
                let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
                loop {
                    let mut line = String::new();
                    match std::io::BufRead::read_line(&mut reader, &mut line) {
                        Ok(0) => break,
                        Ok(_) if line.trim_end().is_empty() => break,
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
                let body = r#"{"hello":"world"}"#;
                let _ = std::io::Write::write_all(
                    &mut stream,
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                );
            }
        });
        let error = match MeiliFs::new(&conn_for(port), "k") {
            Ok(_) => panic!("a non-Meilisearch server was accepted"),
            Err(error) => error,
        };
        assert!(error.contains("isn't Meilisearch"), "got {error}");
    }

    /// Against a **real** Meilisearch, not the stand-in above.
    ///
    /// `#[ignore]`d because it needs one running:
    ///
    /// ```text
    /// EDEN_TEST_MEILI=127.0.0.1:7700 EDEN_TEST_MEILI_KEY=...     /// cargo test -- --ignored real_meili
    /// ```
    ///
    /// Verified against Meilisearch 1.24.
    #[test]
    #[ignore]
    fn a_real_meilisearch_lists_its_indexes_and_documents() {
        let Ok(endpoint) = std::env::var("EDEN_TEST_MEILI") else {
            panic!("set EDEN_TEST_MEILI=host:port");
        };
        let (host, port) = endpoint.rsplit_once(':').expect("host:port");
        let conn = RemoteConnection {
            id: 900_010,
            kind: RemoteKind::Meilisearch,
            host: host.to_string(),
            port: port.parse().expect("a port"),
            https: false,
            ..Default::default()
        };
        let key = std::env::var("EDEN_TEST_MEILI_KEY").unwrap_or_default();

        let mut fs = MeiliFs::new(&conn, &key).expect("connect");
        let top = fs.list("/").expect("list the indexes");
        assert!(!top.is_empty(), "no indexes");
        assert!(top.iter().all(|e| e.is_dir), "an index should be a folder");

        // *Every* index, not just the first: listing only the first one
        // picks whichever sorts earliest, which on a seeded server is the
        // small one - and the paging loop, the whole reason this test is
        // worth running against a real server, then never goes round.
        let names: Vec<String> = top.iter().map(|e| e.name.clone()).collect();
        let mut biggest = 0usize;
        for index in &names {
            let docs = fs.list(&format!("/{index}")).expect("list documents");
            assert!(!docs.is_empty(), "{index} looked empty");
            biggest = biggest.max(docs.len());

            // Every document exactly once - the off-by-one that duplicates
            // or drops one per page.
            let mut seen: Vec<String> = docs.iter().map(|e| e.name.clone()).collect();
            seen.sort();
            let before = seen.len();
            seen.dedup();
            assert_eq!(seen.len(), before, "{index} listed a document twice");

            // And one of them actually opens.
            let mut out = Vec::new();
            fs.download(&format!("/{index}/{}", docs[0].name), &mut out, &mut |_| {})
                .expect("download");
            assert!(!out.is_empty(), "a document in {index} came back empty");
        }
        assert!(
            biggest > PAGE,
            "seed an index with more than {PAGE} documents or this proves \
             nothing about paging - the biggest here had {biggest}"
        );

        assert!(fs.remove_file("/nope/1.json").is_err(), "read-only");
    }
}
