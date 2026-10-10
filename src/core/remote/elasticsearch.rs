//! Elasticsearch and OpenSearch, browsed like a folder tree. **Read-only.**
//!
//! ```text
//! /                       every index       GET /_cat/indices
//! /logs-2026              its documents     POST /logs-2026/_search
//! /logs-2026/<id>.json    one document      GET /logs-2026/_doc/<id>
//! ```
//!
//! OpenSearch is the same API for everything here, so one location type
//! covers both.
//!
//! Three decisions worth knowing:
//!
//! * **Listing uses the scroll API**, not `from`/`size`. Deep offset paging
//!   in Elasticsearch stops entirely at `max_result_window` (10,000 by
//!   default) and gets slower long before that, because every shard has to
//!   build and discard the skipped hits. A scroll is a cursor, the same shape
//!   `mongodb` uses, and it is cleared when the walk finishes rather than
//!   left to time out.
//! * **A document's id is percent-encoded into its file name.** Elasticsearch
//!   allows a `/` in an id and paths here are split on `/`, so an id is
//!   escaped rather than sanitised - sanitising would invent folders and then
//!   fail to fetch the document back.
//! * **Nothing here writes.** Same reasoning as `redis`, `couchdb` and
//!   `mongodb`: a file manager's reflexes are fast and destructive, and a
//!   live cluster has no Recycle Bin.

use super::http_json::{Auth, JsonHttp, file_name_for, id_from_file_name};
use super::webdav::encode_path;
use super::{RemoteConnection, RemoteEntry, RemoteFs};
use serde_json::Value;
use std::io::{Read, Write};

/// Hits asked for in one scroll batch.
const BATCH: usize = 1_000;

/// Stop listing an index after this many documents.
const MAX_DOCS: usize = 20_000;

/// How long the cluster should keep a scroll alive between batches.
const SCROLL_KEEPALIVE: &str = "2m";

pub struct ElasticFs {
    http: JsonHttp,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn read_only() -> String {
    "This Elasticsearch location is read-only.".to_string()
}

/// `/logs/abc.json` → the index and the document it names.
fn split(path: &str) -> (Option<String>, Option<String>) {
    let mut parts = super::segments_of(path).into_iter();
    (
        parts.next(),
        parts.next().as_deref().map(id_from_file_name),
    )
}

/// Seconds since the epoch from a document's own `@timestamp`, when it has
/// one.
///
/// Elasticsearch stores no per-document modification time, but the field is
/// a near-universal convention in the log and metric indices this will
/// mostly be pointed at - so the Modified column is filled where it honestly
/// can be, and left blank otherwise.
fn timestamp_of(source: &Value) -> Option<i64> {
    let value = source.get("@timestamp")?;
    if let Some(millis) = value.as_i64() {
        // Epoch millis if it is plainly too large to be seconds.
        return Some(if millis > 100_000_000_000 {
            millis / 1000
        } else {
            millis
        });
    }
    let text = value.as_str()?;
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|t| t.timestamp())
}

/// The cluster explains itself well; prefer its own reason over a bare
/// status whenever it gives one.
fn detail(value: &Value) -> Option<String> {
    Some(value.get("error")?.get("reason")?.as_str()?.to_string())
}

impl ElasticFs {
    pub fn new(conn: &RemoteConnection, secret: &str) -> Result<Self, String> {
        let fs = ElasticFs {
            http: JsonHttp::new(
                &conn.host,
                conn.port,
                conn.https,
                Auth::Basic {
                    user: conn.username.clone(),
                    secret: secret.to_string(),
                },
            )?
            .with_detail(detail),
        };

        // The root document names the product, so a server that answers but
        // isn't Elasticsearch is reported as such rather than failing later
        // with something cryptic.
        let root = fs.http.get("/", "the server")?;
        let parsed: Value = serde_json::from_str(&root).unwrap_or(Value::Null);
        if parsed.get("cluster_name").is_none() && parsed.get("version").is_none() {
            return Err("That server answered, but it isn't Elasticsearch or OpenSearch.".into());
        }
        Ok(fs)
    }

    /// Every index, from `_cat`. Hidden and system indices are left out,
    /// which is `_cat`'s own default.
    fn indices(&self) -> Result<Vec<String>, String> {
        let body = self.http.get("/_cat/indices?format=json&h=index", "the index list")?;
        let parsed: Vec<Value> = serde_json::from_str(&body)
            .map_err(|e| format!("The index list could not be read ({e})."))?;
        let mut names: Vec<String> = parsed
            .iter()
            .filter_map(|entry| Some(entry.get("index")?.as_str()?.to_string()))
            .collect();
        names.sort();
        Ok(names)
    }

    /// Every document in an index, up to `MAX_DOCS`, following the scroll
    /// until it runs out.
    fn documents(&self, index: &str) -> Result<Vec<Value>, String> {
        let first = self.http.send(
            "POST",
            &format!("/{}/_search?scroll={SCROLL_KEEPALIVE}", encode_path(index)),
            Some(format!("{{\"size\":{BATCH},\"sort\":[\"_doc\"]}}")),
            "the documents",
        )?;
        let mut page: Value = serde_json::from_str(&first)
            .map_err(|e| format!("The search result could not be read ({e})."))?;

        let mut out: Vec<Value> = hits(&page);
        let mut scroll = scroll_id(&page);

        while out.len() < MAX_DOCS {
            let Some(id) = scroll.clone() else { break };
            let body = serde_json::json!({"scroll": SCROLL_KEEPALIVE, "scroll_id": id});
            let next = self.http.send(
                "POST",
                "/_search/scroll",
                Some(body.to_string()),
                "the next batch",
            )?;
            page = serde_json::from_str(&next)
                .map_err(|e| format!("The next batch could not be read ({e})."))?;
            let batch = hits(&page);
            if batch.is_empty() {
                break;
            }
            out.extend(batch);
            scroll = scroll_id(&page);
        }

        // A scroll holds search context on every shard until it expires;
        // this is read-only but not inconsiderate.
        if let Some(id) = scroll {
            let body = serde_json::json!({ "scroll_id": [id] });
            let _ = self.http.send("DELETE", "/_search/scroll", Some(body.to_string()), "the scroll");
        }

        out.truncate(MAX_DOCS);
        Ok(out)
    }
}

fn hits(page: &Value) -> Vec<Value> {
    page.get("hits")
        .and_then(|hits| hits.get("hits"))
        .and_then(|hits| hits.as_array())
        .cloned()
        .unwrap_or_default()
}

fn scroll_id(page: &Value) -> Option<String> {
    Some(page.get("_scroll_id")?.as_str()?.to_string())
}

impl RemoteFs for ElasticFs {
    fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>, String> {
        match split(dir) {
            (None, _) => Ok(self
                .indices()?
                .into_iter()
                .map(|name| RemoteEntry {
                    name,
                    is_dir: true,
                    size: None,
                    modified: None,
                })
                .collect()),
            (Some(index), None) => {
                let documents = self.documents(&index)?;
                Ok(documents
                    .iter()
                    .filter_map(|hit| {
                        let id = hit.get("_id")?.as_str()?;
                        let source = hit.get("_source");
                        Some(RemoteEntry {
                            name: file_name_for(id),
                            is_dir: false,
                            // What the document takes as JSON, which is what
                            // opening it produces.
                            size: source.map(|s| s.to_string().len() as u64),
                            modified: source.and_then(timestamp_of),
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
        let body = self.http.get(
            &format!("/{}/_doc/{}", encode_path(&index), encode_path(&id)),
            "the document",
        )?;
        let pretty = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| serde_json::to_string_pretty(&value).ok())
            .unwrap_or(body);
        to.write_all(pretty.as_bytes()).map_err(err)?;
        progress(pretty.len() as u64);
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
    use serde_json::json;


    /// A stand-in cluster speaking just enough of the search API to drive a
    /// real `ElasticFs` over a real socket.
    ///
    /// Same approach as `redis`, `couchdb` and `mongodb`. What a unit test
    /// can't reach here is the scroll loop, the auth header, and the methods
    /// and bodies the client actually sends - all of which this records so a
    /// test can assert on them.
    mod fake {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::{TcpListener, TcpStream};
        use std::sync::{Arc, Mutex};

        pub struct Seen {
            /// `METHOD path` for every request.
            pub calls: Vec<String>,
            pub auth: Option<String>,
            pub bodies: Vec<String>,
        }

        /// Starts the stand-in. `ids` is the whole of `logs`, handed out
        /// `batch` at a time so the scroll loop has to go round.
        pub fn start(ids: Vec<String>, batch: usize) -> (u16, Arc<Mutex<Seen>>) {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().expect("addr").port();
            let seen = Arc::new(Mutex::new(Seen {
                calls: Vec::new(),
                auth: None,
                bodies: Vec::new(),
            }));
            let recorder = Arc::clone(&seen);
            let served = Arc::new(Mutex::new(0usize));

            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { break };
                    let ids = ids.clone();
                    let recorder = Arc::clone(&recorder);
                    let served = Arc::clone(&served);
                    std::thread::spawn(move || serve(&mut stream, &ids, batch, &recorder, &served));
                }
            });
            (port, seen)
        }

        fn serve(
            stream: &mut TcpStream,
            ids: &[String],
            batch: usize,
            seen: &Arc<Mutex<Seen>>,
            served: &Arc<Mutex<usize>>,
        ) {
            let Ok(clone) = stream.try_clone() else { return };
            let mut reader = BufReader::new(clone);
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                return;
            }
            let mut words = request_line.split_whitespace();
            let method = words.next().unwrap_or("GET").to_string();
            let path = words.next().unwrap_or("/").to_string();

            let mut auth = None;
            let mut length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 {
                    break;
                }
                if header.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = header.trim().split_once(": ") {
                    if name.eq_ignore_ascii_case("authorization") {
                        auth = Some(value.to_string());
                    } else if name.eq_ignore_ascii_case("content-length") {
                        length = value.parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0u8; length];
            if length > 0 {
                let _ = reader.read_exact(&mut body);
            }
            let body = String::from_utf8_lossy(&body).into_owned();

            if let Ok(mut seen) = seen.lock() {
                seen.calls.push(format!("{method} {path}"));
                if auth.is_some() {
                    seen.auth = auth;
                }
                if !body.is_empty() {
                    seen.bodies.push(body.clone());
                }
            }

            let payload = answer(&path, ids, batch, served);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }

        fn answer(path: &str, ids: &[String], batch: usize, served: &Arc<Mutex<usize>>) -> String {
            if path == "/" {
                return r#"{"name":"node-1","cluster_name":"test","version":{"number":"8.13.0"},
                           "tagline":"You Know, for Search"}"#
                    .replace('\n', "")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join("");
            }
            if path.starts_with("/_cat/indices") {
                return r#"[{"index":"logs"},{"index":"metrics"}]"#.to_string();
            }
            if path == "/_search/scroll" {
                let mut at = served.lock().expect("lock");
                let taken: Vec<&String> = ids.iter().skip(*at).take(batch).collect();
                *at += taken.len();
                return page(&taken);
            }
            if path.contains("/_search?scroll=") {
                let mut at = served.lock().expect("lock");
                *at = 0;
                let taken: Vec<&String> = ids.iter().take(batch).collect();
                *at = taken.len();
                return page(&taken);
            }
            if path.contains("/_doc/") {
                let id = path.rsplit("/_doc/").next().unwrap_or("");
                return format!(
                    r#"{{"_index":"logs","_id":"{}","found":true,"_source":{{"message":"hello"}}}}"#,
                    decode(id)
                );
            }
            r#"{"acknowledged":true}"#.to_string()
        }

        fn page(ids: &[&String]) -> String {
            let hits: Vec<String> = ids
                .iter()
                .map(|id| {
                    format!(
                        r#"{{"_id":"{id}","_source":{{"message":"hello","@timestamp":"2026-10-10T08:30:00Z"}}}}"#
                    )
                })
                .collect();
            format!(
                r#"{{"_scroll_id":"SCROLL123","hits":{{"hits":[{}]}}}}"#,
                hits.join(",")
            )
        }

        fn decode(value: &str) -> String {
            let bytes = value.as_bytes();
            let mut out = String::new();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'%' && i + 2 < bytes.len() {
                    if let Ok(byte) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                        out.push(byte as char);
                        i += 3;
                        continue;
                    }
                }
                out.push(bytes[i] as char);
                i += 1;
            }
            out
        }
    }

    fn connection(port: u16) -> RemoteConnection {
        RemoteConnection {
            id: 1,
            kind: crate::core::remote::RemoteKind::Elasticsearch,
            host: "127.0.0.1".into(),
            port,
            username: "elastic".into(),
            https: false,
            ..Default::default()
        }
    }

    fn names(entries: &[RemoteEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn a_real_client_lists_indices_documents_and_reads_one() {
        let ids = vec!["abc".to_string(), "def".into()];
        let (port, seen) = fake::start(ids, 10);
        let mut fs = ElasticFs::new(&connection(port), "secret").expect("connect");

        assert_eq!(names(&fs.list("/").expect("indices")), vec!["logs", "metrics"]);

        let documents = fs.list("/logs").expect("documents");
        assert_eq!(names(&documents), vec!["abc.json", "def.json"]);
        // `@timestamp` fills the Modified column where the store itself has
        // no modification time of its own.
        assert_eq!(documents[0].modified, Some(1_791_621_000));
        assert!(documents[0].size.unwrap_or(0) > 0);

        let mut out = Vec::new();
        let mut size = 0u64;
        fs.download("/logs/abc.json", &mut out, &mut |n| size = n)
            .expect("download");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("\"_id\": \"abc\""), "got: {text}");
        assert!(text.contains('\n'), "should be pretty-printed: {text}");
        assert_eq!(size, text.len() as u64);

        let seen = seen.lock().expect("lock");
        assert_eq!(
            seen.auth.as_deref(),
            Some("Basic ZWxhc3RpYzpzZWNyZXQ="),
            "elastic:secret, base64"
        );
    }

    #[test]
    fn listing_scrolls_rather_than_paging_by_offset() {
        // Five documents two at a time: one search plus scrolls.
        let ids: Vec<String> = (0..5).map(|n| format!("doc{n}")).collect();
        let (port, seen) = fake::start(ids, 2);
        let mut fs = ElasticFs::new(&connection(port), "secret").expect("connect");

        let documents = fs.list("/logs").expect("documents");
        assert_eq!(documents.len(), 5, "every batch, each document once");
        let unique: std::collections::HashSet<&str> = names(&documents).into_iter().collect();
        assert_eq!(unique.len(), 5, "a document was listed twice");

        let seen = seen.lock().expect("lock");
        assert!(
            seen.calls.iter().any(|c| c.contains("/_search?scroll=")),
            "the first request must open a scroll: {:?}",
            seen.calls
        );
        assert!(
            seen.calls.iter().filter(|c| c.contains("POST /_search/scroll")).count() >= 2,
            "the scroll must be followed: {:?}",
            seen.calls
        );
        // Offset paging would stop dead at max_result_window.
        assert!(
            !seen.calls.iter().any(|c| c.contains("from=")),
            "should not page by offset: {:?}",
            seen.calls
        );
        // And the scroll must be given back rather than left to expire.
        assert!(
            seen.calls.iter().any(|c| c.starts_with("DELETE /_search/scroll")),
            "the scroll should be cleared: {:?}",
            seen.calls
        );
    }

    #[test]
    fn an_id_with_a_separator_survives_the_round_trip_to_the_server() {
        let (port, seen) = fake::start(vec!["a/b".to_string()], 10);
        let mut fs = ElasticFs::new(&connection(port), "secret").expect("connect");

        let documents = fs.list("/logs").expect("documents");
        assert_eq!(names(&documents), vec!["a%2Fb.json"], "no invented folder");

        let mut out = Vec::new();
        fs.download("/logs/a%2Fb.json", &mut out, &mut |_| {})
            .expect("download");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("\"_id\": \"a/b\""), "got: {text}");

        let seen = seen.lock().expect("lock");
        assert!(
            seen.calls.iter().any(|c| c.contains("/_doc/")),
            "calls: {:?}",
            seen.calls
        );
    }

    #[test]
    fn every_write_is_refused() {
        let (port, _) = fake::start(vec!["abc".to_string()], 10);
        let mut fs = ElasticFs::new(&connection(port), "secret").expect("connect");
        let mut empty = std::io::empty();
        assert!(fs.upload(&mut empty, 0, "/logs/x.json", &mut |_| {}).is_err());
        assert!(fs.mkdir("/logs2").is_err());
        assert!(fs.rename("/logs/a.json", "/logs/b.json").is_err());
        assert!(fs.remove_file("/logs/abc.json").is_err());
        assert!(fs.remove_dir("/logs").is_err());
    }

    #[test]
    fn an_id_round_trips_through_its_file_name() {
        for id in [
            "plain",
            "with space",
            "a/b",
            "a\\b",
            "100%",
            "already.json",
            "mixed/%5C",
        ] {
            assert_eq!(
                id_from_file_name(&file_name_for(id)),
                id,
                "failed for {id:?}"
            );
        }
        // A separator must not survive into the name, or it would invent a
        // folder level that cannot be navigated back out of.
        assert!(!file_name_for("a/b").contains('/'));
        assert_eq!(file_name_for("a/b"), "a%2Fb.json");
    }

    #[test]
    fn a_path_splits_into_its_index_and_document() {
        assert_eq!(split("/"), (None, None));
        assert_eq!(split("/logs-2026"), (Some("logs-2026".into()), None));
        assert_eq!(
            split("/logs-2026/abc.json"),
            (Some("logs-2026".into()), Some("abc".into()))
        );
        assert_eq!(
            split("/logs-2026/a%2Fb.json"),
            (Some("logs-2026".into()), Some("a/b".into())),
            "an escaped separator comes back as the real id"
        );
    }

    #[test]
    fn a_timestamp_field_fills_the_modified_column_when_it_can() {
        // 2026-10-10T08:30:00Z, computed independently rather than read back
        // off the code this is checking.
        const WHEN: i64 = 1_791_621_000;
        assert_eq!(
            timestamp_of(&json!({"@timestamp": "2026-10-10T08:30:00Z"})),
            Some(WHEN)
        );
        // Epoch milliseconds, which is the other common shape.
        assert_eq!(timestamp_of(&json!({"@timestamp": WHEN * 1000})), Some(WHEN));
        // Plain seconds are left alone.
        assert_eq!(timestamp_of(&json!({"@timestamp": WHEN})), Some(WHEN));
        // No field, or one that isn't a time, leaves the column blank rather
        // than inventing a date.
        assert_eq!(timestamp_of(&json!({"message": "hello"})), None);
        assert_eq!(timestamp_of(&json!({"@timestamp": "not a date"})), None);
    }

    #[test]
    fn hits_and_the_scroll_id_are_read_and_an_empty_page_ends_the_walk() {
        let page = json!({
            "_scroll_id": "abc123",
            "hits": {"hits": [{"_id": "1", "_source": {}}, {"_id": "2", "_source": {}}]}
        });
        assert_eq!(hits(&page).len(), 2);
        assert_eq!(scroll_id(&page).as_deref(), Some("abc123"));

        let done = json!({"_scroll_id": "abc123", "hits": {"hits": []}});
        assert!(hits(&done).is_empty());

        // A reply with neither must not panic.
        assert!(hits(&json!({})).is_empty());
        assert_eq!(scroll_id(&json!({})), None);
    }
}
