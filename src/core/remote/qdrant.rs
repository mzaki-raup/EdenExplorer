//! Qdrant, browsed like a folder tree. **Read-only.**
//!
//! ```text
//! /                    every collection   GET  /collections
//! /articles            its points         POST /collections/articles/points/scroll
//! /articles/7.json     one point          GET  /collections/articles/points/7
//! ```
//!
//! Three things worth knowing:
//!
//! * **Listing walks the scroll cursor, and the cursor is not a number.**
//!   `next_page_offset` comes back as whatever type the collection's ids
//!   are: an integer for one collection, a UUID string for the next. It is
//!   carried back as the JSON value it arrived as rather than parsed.
//!   Parsing it as a number would work against every integer-keyed
//!   collection and fail against every UUID-keyed one, which is the kind of
//!   bug that only shows up on somebody else's data.
//! * **Vectors are left out of a listing.** A point's vector is the bulk of
//!   it - a real collection is 768 or 1536 dimensions - so asking for them
//!   just to fill in a Size column would download tens of megabytes per
//!   folder. The column is therefore blank, the same trade `couchdb` makes
//!   for the same reason. **Opening** a point does include its vector,
//!   because a vector point without its vector is not the point.
//! * **A failure is explained in `status.error`**, and the one that matters
//!   most - a missing or wrong API key - is answered in *plain text*, not
//!   JSON. The shared layer already falls back to the status when a body
//!   isn't JSON, so that case stays legible.
//!
//! Nothing here writes, for the same reason as every other store in this
//! directory: a file manager's reflexes are fast and destructive, and a live
//! database has no Recycle Bin.

use super::http_json::{Auth, JsonHttp, file_name_for, id_from_file_name, pretty};
use super::webdav::encode_path;
use super::{RemoteConnection, RemoteEntry, RemoteFs};
use serde_json::{Value, json};
use std::io::{Read, Write};

/// Points asked for in one scroll batch.
const BATCH: usize = 1_000;

/// Stop listing a collection after this many points.
const MAX_POINTS: usize = 20_000;

pub struct QdrantFs {
    http: JsonHttp,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn read_only() -> String {
    "This Qdrant location is read-only.".to_string()
}

/// Qdrant puts its explanation in `status.error` ("Not found: Collection
/// `nope` doesn't exist!"), which beats a bare 404.
fn detail(value: &Value) -> Option<String> {
    Some(value.get("status")?.get("error")?.as_str()?.to_string())
}

/// `/articles/7.json` → the collection and the point it names.
fn split(path: &str) -> (Option<String>, Option<String>) {
    let mut parts = super::segments_of(path).into_iter();
    (
        parts.next(),
        parts.next().as_deref().map(id_from_file_name),
    )
}

/// A point id as a string, however it is typed.
///
/// Qdrant ids are an unsigned integer or a UUID, and `7` and `"7"` name the
/// same point - so a number is rendered plainly rather than with JSON
/// quoting around it.
fn id_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

impl QdrantFs {
    pub fn new(conn: &RemoteConnection, secret: &str) -> Result<Self, String> {
        let fs = QdrantFs {
            http: JsonHttp::new(
                &conn.host,
                conn.port,
                conn.https,
                // Qdrant's own header. It also accepts a bearer token, but
                // `api-key` is what its documentation and every client use.
                Auth::Header {
                    name: "api-key",
                    value: secret.to_string(),
                },
            )?
            .with_detail(detail),
        };

        // The root names the product, so a server that answers but isn't
        // Qdrant is reported as such rather than failing later with
        // something cryptic.
        let root = fs.http.get_json("/", "the server")?;
        if root.get("title").and_then(Value::as_str) != Some("qdrant - vector search engine") {
            return Err("That server answered, but it isn't Qdrant.".into());
        }
        // And this one says the API key was accepted.
        fs.collections()?;
        Ok(fs)
    }

    fn collections(&self) -> Result<Vec<String>, String> {
        let body = self.http.get_json("/collections", "the collection list")?;
        let mut names: Vec<String> = body
            .get("result")
            .and_then(|result| result.get("collections"))
            .and_then(Value::as_array)
            .map(|collections| {
                collections
                    .iter()
                    .filter_map(|c| Some(c.get("name")?.as_str()?.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        Ok(names)
    }

    /// Every point in a collection, up to `MAX_POINTS`, following the scroll
    /// cursor until it runs out.
    fn points(&self, collection: &str) -> Result<Vec<Value>, String> {
        let path = format!("/collections/{}/points/scroll", encode_path(collection));
        let mut out: Vec<Value> = Vec::new();
        let mut offset: Option<Value> = None;

        while out.len() < MAX_POINTS {
            let mut body = json!({
                "limit": BATCH,
                "with_payload": true,
                // See the module docs: a vector is the bulk of a point, and
                // a folder listing must not pull them all down.
                "with_vector": false,
            });
            if let Some(cursor) = &offset {
                body["offset"] = cursor.clone();
            }
            let page = self.http.post_json(&path, body, "the points")?;
            let result = page.get("result");
            let batch: Vec<Value> = result
                .and_then(|result| result.get("points"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if batch.is_empty() {
                break;
            }
            out.extend(batch);

            // Carried back exactly as it arrived - it is an integer for an
            // integer-keyed collection and a UUID string for a UUID-keyed
            // one. `null` means the walk is done.
            offset = match result.and_then(|result| result.get("next_page_offset")) {
                Some(Value::Null) | None => None,
                Some(cursor) => Some(cursor.clone()),
            };
            if offset.is_none() {
                break;
            }
        }

        out.truncate(MAX_POINTS);
        Ok(out)
    }
}

impl RemoteFs for QdrantFs {
    fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>, String> {
        match split(dir) {
            (None, _) => Ok(self
                .collections()?
                .into_iter()
                .map(|name| RemoteEntry {
                    name,
                    is_dir: true,
                    size: None,
                    modified: None,
                })
                .collect()),
            (Some(collection), None) => {
                let points = self.points(&collection)?;
                Ok(points
                    .iter()
                    .filter_map(|point| {
                        Some(RemoteEntry {
                            name: file_name_for(&id_text(point.get("id")?)?),
                            is_dir: false,
                            // Blank on purpose: the file that opens carries
                            // the vector too, and fetching every vector just
                            // to size a column is exactly what the listing
                            // avoids.
                            size: None,
                            // Qdrant keeps no per-point timestamp.
                            modified: None,
                        })
                    })
                    .collect())
            }
            // A point is not a folder.
            _ => Err("Not a folder.".into()),
        }
    }

    fn download(
        &mut self,
        file: &str,
        to: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), String> {
        let (Some(collection), Some(id)) = split(file) else {
            return Err("Not a point.".into());
        };
        let body = self.http.get_json(
            &format!(
                "/collections/{}/points/{}",
                encode_path(&collection),
                encode_path(&id)
            ),
            "the point",
        )?;
        // Unwrap Qdrant's envelope: the point itself is what opening it
        // should show, not `{"result": ..., "status": "ok", "time": ...}`.
        let point = body.get("result").unwrap_or(&body);
        let text = pretty(point);
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
    fn a_path_splits_into_a_collection_and_a_point() {
        assert_eq!(split("/"), (None, None));
        assert_eq!(split("/articles"), (Some("articles".into()), None));
        assert_eq!(
            split("/articles/7.json"),
            (Some("articles".into()), Some("7".into()))
        );
        assert_eq!(
            split("/notes/00000000-0000-0000-0000-000000000003.json"),
            (
                Some("notes".into()),
                Some("00000000-0000-0000-0000-000000000003".into())
            )
        );
    }

    /// `7` and `"7"` name the same point, so a numeric id must not come back
    /// carrying JSON quotes.
    #[test]
    fn a_point_id_is_either_a_number_or_a_uuid() {
        assert_eq!(id_text(&json!(7)).as_deref(), Some("7"));
        assert_eq!(
            id_text(&json!("00000000-0000-0000-0000-000000000003")).as_deref(),
            Some("00000000-0000-0000-0000-000000000003")
        );
        assert_eq!(id_text(&json!(null)), None);
        assert_eq!(id_text(&json!({"a": 1})), None);
    }

    #[test]
    fn the_servers_own_error_is_what_a_failure_reports() {
        let body = json!({"status": {"error": "Not found: Collection `nope` doesn't exist!"}});
        assert_eq!(
            detail(&body).as_deref(),
            Some("Not found: Collection `nope` doesn't exist!")
        );
        // A plain-text refusal (what a missing API key gets) isn't JSON at
        // all - the shared layer falls back to the status for that.
        assert_eq!(detail(&json!({"status": "ok"})), None);
    }

    /// A stand-in Qdrant speaking just enough of the points API to drive a
    /// real `QdrantFs` over a real socket.
    ///
    /// The same approach as every other store here. What a unit test can't
    /// reach is the scroll loop, the `api-key` header, and the bodies the
    /// client actually posts - all recorded so a test can assert on them.
    mod fake {
        use std::io::{BufRead, BufReader, Read as _, Write};
        use std::net::{TcpListener, TcpStream};
        use std::sync::{Arc, Mutex};

        pub struct Seen {
            pub calls: Vec<String>,
            pub api_key: Option<String>,
            pub bodies: Vec<String>,
        }

        /// `ids` is the whole of the collection, handed out `batch` at a
        /// time so the scroll loop has to go round. Ids are strings here on
        /// purpose: a cursor that only works for integers is the bug this
        /// exists to catch.
        pub fn start(ids: Vec<String>, batch: usize) -> (u16, Arc<Mutex<Seen>>) {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().expect("addr").port();
            let seen = Arc::new(Mutex::new(Seen {
                calls: Vec::new(),
                api_key: None,
                bodies: Vec::new(),
            }));
            let recorder = Arc::clone(&seen);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { break };
                    let ids = ids.clone();
                    let recorder = Arc::clone(&recorder);
                    std::thread::spawn(move || serve(stream, ids, batch, recorder));
                }
            });
            (port, seen)
        }

        fn serve(stream: TcpStream, ids: Vec<String>, batch: usize, seen: Arc<Mutex<Seen>>) {
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

                let mut api_key = None;
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
                    if let Some(value) = lower.strip_prefix("api-key:") {
                        api_key = Some(header[header.len() - value.trim().len()..].to_string());
                    }
                    if let Some(value) = lower.strip_prefix("content-length:") {
                        content_length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body_text = String::new();
                if content_length > 0 {
                    let mut raw = vec![0u8; content_length];
                    let _ = reader.read_exact(&mut raw);
                    body_text = String::from_utf8_lossy(&raw).to_string();
                }

                {
                    let mut seen = seen.lock().expect("lock");
                    seen.calls.push(format!("{method} {path}"));
                    if api_key.is_some() {
                        seen.api_key = api_key.clone();
                    }
                    if !body_text.is_empty() {
                        seen.bodies.push(body_text.clone());
                    }
                }

                let body = if path == "/" {
                    r#"{"title":"qdrant - vector search engine","version":"1.16.1"}"#.to_string()
                } else if path == "/collections" {
                    r#"{"result":{"collections":[{"name":"articles"}]},"status":"ok"}"#.to_string()
                } else if path.ends_with("/points/scroll") {
                    // The cursor is the id to start at, carried as a string.
                    let request: serde_json::Value =
                        serde_json::from_str(&body_text).unwrap_or(serde_json::Value::Null);
                    let start = request
                        .get("offset")
                        .and_then(|o| o.as_str())
                        .and_then(|o| ids.iter().position(|id| id == o))
                        .unwrap_or(0);
                    let end = (start + batch).min(ids.len());
                    let points: Vec<String> = ids[start..end]
                        .iter()
                        .map(|id| format!(r#"{{"id":"{id}","payload":{{"n":"{id}"}}}}"#))
                        .collect();
                    let next = match ids.get(end) {
                        Some(id) => format!("\"{id}\""),
                        None => "null".to_string(),
                    };
                    format!(
                        r#"{{"result":{{"points":[{}],"next_page_offset":{next}}},"status":"ok"}}"#,
                        points.join(",")
                    )
                } else if path.contains("/points/") {
                    let id = path.rsplit('/').next().unwrap_or("");
                    format!(
                        r#"{{"result":{{"id":"{id}","payload":{{"n":"{id}"}},"vector":[0.5,0.5]}},"status":"ok"}}"#
                    )
                } else {
                    let response = "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\n\
                         Content-Length: 44\r\nConnection: keep-alive\r\n\r\n\
                         {\"status\":{\"error\":\"Not found: nope\"}}";
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
            id: 900_011,
            kind: RemoteKind::Qdrant,
            host: "127.0.0.1".into(),
            port,
            https: false,
            ..Default::default()
        }
    }

    #[test]
    fn collections_list_as_folders() {
        let (port, _seen) = fake::start(vec!["a".into()], BATCH);
        let mut fs = QdrantFs::new(&conn_for(port), "k").expect("connect");
        let top = fs.list("/").expect("list the root");
        assert_eq!(top.len(), 1);
        assert!(top[0].is_dir);
        assert_eq!(top[0].name, "articles");
    }

    /// The scroll has to go round, the cursor has to survive being a
    /// *string*, and the `api-key` header has to be sent - none of which a
    /// unit test reaches.
    #[test]
    fn points_are_scrolled_with_a_cursor_that_is_not_a_number() {
        let ids: Vec<String> = (0..2_500).map(|i| format!("id-{i:05}")).collect();
        let (port, seen) = fake::start(ids.clone(), BATCH);
        let mut fs = QdrantFs::new(&conn_for(port), "eden-test-api-key").expect("connect");
        let points = fs.list("/articles").expect("list points");
        assert_eq!(points.len(), 2_500);

        // Every point exactly once - the off-by-one a cursor walk invites.
        let mut names: Vec<String> = points.iter().map(|e| e.name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 2_500, "a point was listed twice");

        let seen = seen.lock().expect("lock");
        assert_eq!(
            seen.api_key.as_deref(),
            Some("eden-test-api-key"),
            "Qdrant takes its own api-key header"
        );
        let scrolls = seen.calls.iter().filter(|c| c.contains("scroll")).count();
        assert_eq!(scrolls, 3, "2500 points in batches of 1000");

        // No request asks for vectors - the whole reason a folder listing
        // stays small.
        assert!(
            seen.bodies.iter().all(|b| b.contains("\"with_vector\":false")),
            "a listing asked for vectors: {:?}",
            seen.bodies
        );
        // And the second request really carried the cursor the first
        // returned, rather than paging by a number it invented.
        assert!(
            seen.bodies.iter().any(|b| b.contains("\"offset\":\"id-01000\"")
                || b.contains("\"offset\": \"id-01000\"")),
            "the cursor wasn't carried back: {:?}",
            seen.bodies
        );
    }

    #[test]
    fn a_point_opens_without_the_envelope_around_it() {
        let (port, _seen) = fake::start(vec!["id-1".into()], BATCH);
        let mut fs = QdrantFs::new(&conn_for(port), "k").expect("connect");
        let mut out = Vec::new();
        fs.download("/articles/id-1.json", &mut out, &mut |_| {})
            .expect("download");
        let text = String::from_utf8(out).expect("utf-8");
        assert!(text.contains("\"id\": \"id-1\""), "got {text}");
        // Opening a point shows the point, not Qdrant's `{result, status}`
        // wrapper around it.
        assert!(!text.contains("\"status\""), "the envelope leaked: {text}");
        // And it does carry the vector, unlike the listing.
        assert!(text.contains("vector"), "the vector is missing: {text}");
    }

    #[test]
    fn every_write_is_refused() {
        let (port, _seen) = fake::start(vec!["id-1".into()], BATCH);
        let mut fs = QdrantFs::new(&conn_for(port), "k").expect("connect");
        assert!(fs.mkdir("/new").is_err());
        assert!(fs.rename("/articles/a.json", "/articles/b.json").is_err());
        assert!(fs.remove_file("/articles/a.json").is_err());
        assert!(fs.remove_dir("/articles").is_err());
        assert!(
            fs.upload(&mut std::io::empty(), 0, "/articles/a.json", &mut |_| {})
                .is_err()
        );
    }

    /// Against a **real** Qdrant, not the stand-in above.
    ///
    /// `#[ignore]`d because it needs one running:
    ///
    /// ```text
    /// EDEN_TEST_QDRANT=127.0.0.1:6333 EDEN_TEST_QDRANT_KEY=... \
    /// cargo test -- --ignored real_qdrant
    /// ```
    ///
    /// Verified against Qdrant 1.16.1.
    #[test]
    #[ignore]
    fn a_real_qdrant_scrolls_its_collections() {
        let Ok(endpoint) = std::env::var("EDEN_TEST_QDRANT") else {
            panic!("set EDEN_TEST_QDRANT=host:port");
        };
        let (host, port) = endpoint.rsplit_once(':').expect("host:port");
        let conn = RemoteConnection {
            id: 900_011,
            kind: RemoteKind::Qdrant,
            host: host.to_string(),
            port: port.parse().expect("a port"),
            https: false,
            ..Default::default()
        };
        let key = std::env::var("EDEN_TEST_QDRANT_KEY").unwrap_or_default();

        let mut fs = QdrantFs::new(&conn, &key).expect("connect");
        let top = fs.list("/").expect("list the collections");
        assert!(!top.is_empty(), "no collections");

        // Every collection, so both an integer-keyed and a UUID-keyed one
        // are walked - the cursor's type is the thing most likely to break
        // and the one a single collection would hide.
        let names: Vec<String> = top.iter().map(|e| e.name.clone()).collect();
        let mut biggest = 0usize;
        for collection in &names {
            let points = fs.list(&format!("/{collection}")).expect("list points");
            assert!(!points.is_empty(), "{collection} looked empty");
            biggest = biggest.max(points.len());

            let mut seen: Vec<String> = points.iter().map(|e| e.name.clone()).collect();
            seen.sort();
            let before = seen.len();
            seen.dedup();
            assert_eq!(seen.len(), before, "{collection} listed a point twice");

            let mut out = Vec::new();
            fs.download(
                &format!("/{collection}/{}", points[0].name),
                &mut out,
                &mut |_| {},
            )
            .expect("download");
            let text = String::from_utf8(out).expect("utf-8");
            assert!(text.contains("vector"), "no vector in {collection}: {text}");
        }
        assert!(
            biggest > BATCH,
            "seed a collection with more than {BATCH} points or this proves \
             nothing about the scroll - the biggest here had {biggest}"
        );

        assert!(fs.remove_file("/articles/1.json").is_err(), "read-only");
    }
}
