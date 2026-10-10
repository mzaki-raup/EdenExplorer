//! CouchDB, browsed like a folder tree. **Read-only.**
//!
//! CouchDB is a document store, so the mapping is more direct than the other
//! non-filesystem locations here: a document really is a file.
//!
//! ```text
//! /                     every database       (GET /_all_dbs)
//! /mydb                 its documents        (GET /mydb/_all_docs)
//! /mydb/alice.json      one document         (GET /mydb/alice)
//! /mydb/_design/views.json   a design document
//! ```
//!
//! Three decisions worth knowing before changing anything here:
//!
//! * **Documents are named `<id>.json`.** The id is what CouchDB stores; the
//!   extension is added so the preview pane and Quick Look recognise the
//!   content, which they do by extension. It round-trips exactly - one
//!   trailing `.json` is stripped to get the id back - so a document really
//!   called `notes.json` lists as `notes.json.json` and still opens.
//! * **`_design` and `_local` are folders.** They are the only ids CouchDB
//!   allows a `/` in, and paths here are split on `/`, so giving them a level
//!   of their own is both necessary and what CouchDB's own interface does.
//! * **Nothing here writes.** Same reasoning as `redis`: a file manager's
//!   reflexes are fast and destructive - select-all then Delete is muscle
//!   memory - and a live database has no Recycle Bin and no undo. Every
//!   mutating method returns an error saying so.

use super::http_json::{Auth, JsonHttp};
use super::webdav::encode_path;
use super::{RemoteConnection, RemoteEntry, RemoteFs};
use std::io::{Read, Write};

/// Documents asked for in one request. CouchDB pages by key, not offset, so
/// this is a round-trip size rather than a limit on what can be reached.
const PAGE: usize = 1_000;

/// Stop listing a database after this many documents. A real one can hold
/// millions, and a file list that never finishes is worse than a capped one.
const MAX_DOCS: usize = 20_000;

/// The ids CouchDB allows a `/` in. Shown as folders.
const PREFIX_FOLDERS: [&str; 2] = ["_design", "_local"];

pub struct CouchFs {
    http: JsonHttp,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn read_only() -> String {
    "This CouchDB location is read-only.".to_string()
}

/// `alice` → `alice.json`, so the preview pane recognises the content.
fn file_name_for(id: &str) -> String {
    format!("{id}.json")
}

/// The inverse: one trailing `.json` is the one this module added.
fn id_from_file_name(name: &str) -> String {
    name.strip_suffix(".json").unwrap_or(name).to_string()
}

/// Splits a path into its database and whatever is below it.
fn split(path: &str) -> (Option<String>, Vec<String>) {
    let mut segments = super::segments_of(path);
    if segments.is_empty() {
        return (None, Vec::new());
    }
    let database = segments.remove(0);
    (Some(database), segments)
}

/// The document id a path below a database names, or `None` when it names a
/// folder rather than a document.
fn document_id(rest: &[String]) -> Option<String> {
    match rest {
        [one] if !PREFIX_FOLDERS.contains(&one.as_str()) => Some(id_from_file_name(one)),
        [prefix, name] if PREFIX_FOLDERS.contains(&prefix.as_str()) => {
            Some(format!("{prefix}/{}", id_from_file_name(name)))
        }
        _ => None,
    }
}

/// Percent-encodes a query-string value, so JSON survives the URL.
fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Turns a flat list of ids into one level of a tree: plain ids become files,
/// and anything under a `_design`/`_local` prefix becomes - or goes inside -
/// a folder of that name.
fn entries_for(ids: &[String], inside: Option<&str>) -> Vec<RemoteEntry> {
    let mut entries: Vec<RemoteEntry> = Vec::new();
    let mut folders: Vec<&str> = Vec::new();

    for id in ids {
        match (id.split_once('/'), inside) {
            // Inside `_design`: that prefix's own documents, and nothing else.
            (Some((prefix, name)), Some(wanted)) if prefix == wanted => {
                entries.push(RemoteEntry {
                    name: file_name_for(name),
                    is_dir: false,
                    size: None,
                    modified: None,
                });
            }
            (_, Some(_)) => {}
            // At the top of a database, a prefixed id contributes its folder
            // once, however many documents share it.
            (Some((prefix, _)), None) => {
                if PREFIX_FOLDERS.contains(&prefix) && !folders.contains(&prefix) {
                    folders.push(prefix);
                    entries.push(RemoteEntry {
                        name: prefix.to_string(),
                        is_dir: true,
                        size: None,
                        modified: None,
                    });
                }
            }
            (None, None) => entries.push(RemoteEntry {
                name: file_name_for(id),
                is_dir: false,
                // `_all_docs` doesn't report a document's size, and asking for
                // it (`include_docs=true`) would download every document just
                // to fill in a column. `RemoteEntry`'s fields are optional
                // for exactly this - Redis leaves `modified` blank for the
                // same reason.
                size: None,
                modified: None,
            }),
        }
    }
    entries
}

impl CouchFs {
    pub fn new(conn: &RemoteConnection, secret: &str) -> Result<Self, String> {
        let fs = CouchFs {
            http: JsonHttp::new(
                &conn.host,
                conn.port,
                conn.https,
                Auth::Basic {
                    user: conn.username.clone(),
                    secret: secret.to_string(),
                },
            )?,
        };

        // Two checks, because they fail for different reasons and a user can
        // act on each: the welcome document says this is reachable and really
        // is CouchDB, and listing the databases says the sign-in worked.
        let welcome = fs.http.get("/", "the server")?;
        if !welcome.contains("\"couchdb\"") {
            return Err("That server answered, but it isn't CouchDB.".into());
        }
        fs.http.get("/_all_dbs", "the database list")?;
        Ok(fs)
    }

    fn databases(&self) -> Result<Vec<String>, String> {
        let body = self.http.get("/_all_dbs", "the database list")?;
        serde_json::from_str::<Vec<String>>(&body)
            .map_err(|e| format!("The database list could not be read ({e})."))
    }

    /// One page of `_all_docs`: the ids, and the id the next page starts at.
    ///
    /// Paged by `startkey` rather than `skip`, because CouchDB walks every
    /// skipped row - offset paging gets slower the further in it goes, while
    /// a start key is a seek into the B-tree.
    fn document_page(
        &self,
        database: &str,
        start: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), String> {
        let mut query = format!("limit={}", PAGE + 1);
        if let Some(start) = start {
            // The server parses `startkey` as JSON, so it is sent as a JSON
            // string and then percent-encoded.
            let as_json = serde_json::to_string(start).map_err(err)?;
            query.push_str(&format!("&startkey={}", encode_query(&as_json)));
        }
        let path = format!("/{}/_all_docs?{query}", encode_path(database));
        let body = self.http.get(&path, "the document list")?;

        let parsed: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| format!("The document list could not be read ({e})."))?;
        let rows = parsed
            .get("rows")
            .and_then(|rows| rows.as_array())
            .ok_or_else(|| "The document list was not in the expected shape.".to_string())?;

        let mut ids: Vec<String> = rows
            .iter()
            .filter_map(|row| Some(row.get("id")?.as_str()?.to_string()))
            .collect();
        // The extra row belongs to the next page, not this one.
        let next = (ids.len() > PAGE).then(|| ids.remove(PAGE));
        Ok((ids, next))
    }

    /// Every document id in a database, up to `MAX_DOCS`.
    fn document_ids(&self, database: &str) -> Result<Vec<String>, String> {
        let mut ids = Vec::new();
        let mut start: Option<String> = None;
        loop {
            let (page, next) = self.document_page(database, start.as_deref())?;
            ids.extend(page);
            match next {
                Some(next) if ids.len() < MAX_DOCS => start = Some(next),
                _ => break,
            }
        }
        ids.truncate(MAX_DOCS);
        Ok(ids)
    }
}

impl RemoteFs for CouchFs {
    fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>, String> {
        let (database, rest) = split(dir);
        let Some(database) = database else {
            return Ok(self
                .databases()?
                .into_iter()
                .map(|name| RemoteEntry {
                    name,
                    is_dir: true,
                    size: None,
                    modified: None,
                })
                .collect());
        };

        let inside = match rest.as_slice() {
            [] => None,
            [prefix] if PREFIX_FOLDERS.contains(&prefix.as_str()) => Some(prefix.as_str()),
            // A document is not a folder.
            _ => return Err("Not a folder.".into()),
        };

        let ids = self.document_ids(&database)?;
        Ok(entries_for(&ids, inside))
    }

    fn download(
        &mut self,
        file: &str,
        to: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), String> {
        let (database, rest) = split(file);
        let (Some(database), Some(id)) = (database, document_id(&rest)) else {
            return Err("Not a document.".into());
        };
        let path = format!("/{}/{}", encode_path(&database), encode_path(&id));
        let body = self.http.get(&path, "the document")?;

        // Pretty-printed: a stored document is usually one long line, and
        // reading it is the whole point of opening it here.
        let pretty = serde_json::from_str::<serde_json::Value>(&body)
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
    use crate::core::remote::RemoteKind;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn names(entries: &[RemoteEntry]) -> Vec<(&str, bool)> {
        entries
            .iter()
            .map(|e| (e.name.as_str(), e.is_dir))
            .collect()
    }


    /// A stand-in CouchDB speaking just enough HTTP to drive a real
    /// `CouchFs` over a real socket.
    ///
    /// Same approach as `redis`'s fake server, and for the same reason: the
    /// parts worth testing here are the wire shape, the `startkey` paging
    /// loop and the auth header - none of which a unit test of the path
    /// helpers would reach, and all of which run in CI with nothing
    /// installed.
    mod fake {
        use std::io::{BufRead, BufReader, Write};
        use std::net::{TcpListener, TcpStream};
        use std::sync::{Arc, Mutex};

        pub struct Seen {
            pub paths: Vec<String>,
            pub auth: Option<String>,
        }

        /// Starts the stand-in on a free port. `docs` is every document id
        /// in `mydb`, in key order - the order a real `_all_docs` returns.
        pub fn start(docs: Vec<String>) -> (u16, Arc<Mutex<Seen>>) {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().expect("addr").port();
            let seen = Arc::new(Mutex::new(Seen {
                paths: Vec::new(),
                auth: None,
            }));
            let recorder = Arc::clone(&seen);

            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { break };
                    let docs = docs.clone();
                    let recorder = Arc::clone(&recorder);
                    if !serve_one(&mut stream, &docs, &recorder) {
                        break;
                    }
                }
            });
            (port, seen)
        }

        /// Handles one request. Returns false once the client has gone.
        fn serve_one(
            stream: &mut TcpStream,
            docs: &[String],
            seen: &Arc<Mutex<Seen>>,
        ) -> bool {
            let mut reader = BufReader::new(match stream.try_clone() {
                Ok(c) => c,
                Err(_) => return false,
            });
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                return true;
            }
            let path = request_line.split_whitespace().nth(1).unwrap_or("/").to_string();

            let mut auth = None;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 {
                    break;
                }
                if header.trim().is_empty() {
                    break;
                }
                // The `http` crate normalises header names to lower case on
                // the wire, so match without caring either way.
                if let Some((name, value)) = header.trim().split_once(": ")
                    && name.eq_ignore_ascii_case("authorization")
                {
                    auth = Some(value.to_string());
                }
            }

            if let Ok(mut seen) = seen.lock() {
                seen.paths.push(path.clone());
                if auth.is_some() {
                    seen.auth = auth;
                }
            }

            let body = body_for(&path, docs);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            true
        }

        fn body_for(path: &str, docs: &[String]) -> String {
            if path == "/" {
                return r#"{"couchdb":"Welcome","version":"3.3.3"}"#.to_string();
            }
            if path.starts_with("/_all_dbs") {
                return r#"["mydb","_users"]"#.to_string();
            }
            if let Some(query) = path.strip_prefix("/mydb/_all_docs") {
                // Everything from `startkey` on, capped at the page size.
                let parts: Vec<&str> = query.trim_start_matches('?').split('&').collect();
                let start = parts
                    .iter()
                    .find_map(|part| part.strip_prefix("startkey="))
                    .map(|value| decode(value).trim_matches('"').to_string());
                // Honoured, like a real server would: the client's whole
                // "there is another page" rule depends on getting back
                // exactly as many rows as it asked for.
                let limit: usize = parts
                    .iter()
                    .find_map(|part| part.strip_prefix("limit="))
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(docs.len());
                let from = match start {
                    Some(key) => docs.iter().position(|id| *id >= key).unwrap_or(docs.len()),
                    None => 0,
                };
                let rows: Vec<String> = docs[from..]
                    .iter()
                    .take(limit)
                    .map(|id| format!(r#"{{"id":"{id}","key":"{id}","value":{{"rev":"1-a"}}}}"#))
                    .collect();
                return format!(
                    r#"{{"total_rows":{},"offset":{from},"rows":[{}]}}"#,
                    docs.len(),
                    rows.join(",")
                );
            }
            // A document: echo its id back, unformatted, on one line.
            let id = path.trim_start_matches("/mydb/");
            format!(r#"{{"_id":"{}","_rev":"1-a","hello":"world"}}"#, decode(id))
        }

        fn decode(value: &str) -> String {
            let bytes = value.as_bytes();
            let mut out = Vec::with_capacity(bytes.len());
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'%' && i + 2 < bytes.len() {
                    if let Ok(byte) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                        out.push(byte);
                        i += 3;
                        continue;
                    }
                }
                out.push(bytes[i]);
                i += 1;
            }
            String::from_utf8_lossy(&out).into_owned()
        }
    }

    fn connection(port: u16) -> RemoteConnection {
        RemoteConnection {
            id: 1,
            kind: RemoteKind::CouchDb,
            host: "127.0.0.1".into(),
            port,
            username: "admin".into(),
            https: false,
            ..Default::default()
        }
    }

    #[test]
    fn a_real_client_lists_databases_documents_and_reads_one() {
        let docs = vec!["_design/views".to_string(), "alice".into(), "bob".into()];
        let (port, seen) = fake::start(docs);
        let mut fs = CouchFs::new(&connection(port), "secret").expect("connect");

        // The root is the database list.
        let databases = fs.list("/").expect("list databases");
        assert_eq!(
            names(&databases),
            vec![("mydb", true), ("_users", true)],
            "every database is a folder"
        );

        // A database is its documents, with `_design` lifted into a folder.
        let documents = fs.list("/mydb").expect("list documents");
        assert_eq!(
            names(&documents),
            vec![("_design", true), ("alice.json", false), ("bob.json", false)]
        );

        // And a document reads back pretty-printed.
        let mut out = Vec::new();
        let mut total = 0u64;
        fs.download("/mydb/alice.json", &mut out, &mut |n| total = n)
            .expect("download");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("\"_id\": \"alice\""), "got: {text}");
        assert!(
            text.contains('\n'),
            "a stored document is one long line; it should be pretty-printed here: {text}"
        );
        assert_eq!(total, text.len() as u64);

        // The request actually carried the sign-in.
        let seen = seen.lock().expect("lock");
        assert_eq!(
            seen.auth.as_deref(),
            Some("Basic YWRtaW46c2VjcmV0"),
            "admin:secret, base64"
        );
    }

    #[test]
    fn a_design_document_is_fetched_under_its_real_id() {
        let docs = vec!["_design/views".to_string()];
        let (port, seen) = fake::start(docs);
        let mut fs = CouchFs::new(&connection(port), "secret").expect("connect");

        let inside = fs.list("/mydb/_design").expect("list design docs");
        assert_eq!(names(&inside), vec![("views.json", false)]);

        let mut out = Vec::new();
        fs.download("/mydb/_design/views.json", &mut out, &mut |_| {})
            .expect("download");
        // The `/` in the id must survive into the request path, not be
        // escaped into one segment.
        let seen = seen.lock().expect("lock");
        assert!(
            seen.paths.iter().any(|p| p == "/mydb/_design/views"),
            "asked for: {:?}",
            seen.paths
        );
    }

    #[test]
    fn a_long_database_is_paged_by_start_key_not_by_offset() {
        // Deliberately more than one page of the real `PAGE`, so the loop
        // is driven by the constant that actually ships rather than a
        // test-only one. Zero-padded so the ids sort the way CouchDB
        // returns them.
        let total = PAGE + PAGE / 2;
        let docs: Vec<String> = (0..total).map(|n| format!("doc{n:06}")).collect();
        let (port, seen) = fake::start(docs.clone());
        let mut fs = CouchFs::new(&connection(port), "secret").expect("connect");

        let documents = fs.list("/mydb").expect("list");
        assert_eq!(
            documents.len(),
            total,
            "every page's documents, each exactly once"
        );
        assert_eq!(documents.first().map(|e| e.name.as_str()), Some("doc000000.json"));
        assert_eq!(
            documents.last().map(|e| e.name.as_str()),
            Some(format!("doc{:06}.json", total - 1)).as_deref()
        );
        // The row that told the client there was more must not also appear as
        // a document - an off-by-one here duplicates one row per page.
        let unique: std::collections::HashSet<&str> =
            documents.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(unique.len(), documents.len(), "a document was listed twice");

        let seen = seen.lock().expect("lock");
        let listings: Vec<&String> = seen
            .paths
            .iter()
            .filter(|p| p.starts_with("/mydb/_all_docs"))
            .collect();
        assert!(listings.len() > 1, "expected more than one page: {listings:?}");
        assert!(
            listings.iter().skip(1).all(|p| p.contains("startkey=")),
            "later pages must seek by key, not skip: {listings:?}"
        );
        assert!(
            !listings.iter().any(|p| p.contains("skip=")),
            "offset paging walks every skipped row: {listings:?}"
        );
    }

    #[test]
    fn every_write_is_refused() {
        let (port, _) = fake::start(vec!["alice".to_string()]);
        let mut fs = CouchFs::new(&connection(port), "secret").expect("connect");

        let mut empty = std::io::empty();
        assert!(fs.upload(&mut empty, 0, "/mydb/x.json", &mut |_| {}).is_err());
        assert!(fs.mkdir("/mydb2").is_err());
        assert!(fs.rename("/mydb/a.json", "/mydb/b.json").is_err());
        assert!(fs.remove_file("/mydb/alice.json").is_err());
        assert!(fs.remove_dir("/mydb").is_err());
    }

    #[test]
    fn a_document_name_round_trips_through_its_file_name() {
        assert_eq!(file_name_for("alice"), "alice.json");
        assert_eq!(id_from_file_name("alice.json"), "alice");
        // A document whose id already ends in `.json` gains a second one and
        // still comes back exactly as stored.
        assert_eq!(file_name_for("notes.json"), "notes.json.json");
        assert_eq!(id_from_file_name("notes.json.json"), "notes.json");
        assert_eq!(id_from_file_name("alice"), "alice");
    }

    #[test]
    fn a_path_splits_into_its_database_and_what_is_below_it() {
        assert_eq!(split("/"), (None, Vec::new()));
        assert_eq!(split("/mydb"), (Some("mydb".into()), Vec::new()));
        assert_eq!(
            split("/mydb/alice.json"),
            (Some("mydb".into()), ids(&["alice.json"]))
        );
        assert_eq!(
            split("/mydb/_design/views.json"),
            (Some("mydb".into()), ids(&["_design", "views.json"]))
        );
    }

    #[test]
    fn a_design_document_keeps_the_slash_in_its_real_id() {
        // Two path segments, one id with a `/` in it - the whole reason
        // `_design` is a folder here.
        assert_eq!(
            document_id(&ids(&["_design", "views.json"])).as_deref(),
            Some("_design/views")
        );
        assert_eq!(document_id(&ids(&["alice.json"])).as_deref(), Some("alice"));
        // A folder is not a document.
        assert_eq!(document_id(&ids(&["_design"])), None);
        assert_eq!(document_id(&[]), None);
    }

    #[test]
    fn the_top_of_a_database_lists_documents_and_one_folder_per_prefix() {
        let all = ids(&[
            "_design/auth",
            "_design/views",
            "_local/checkpoint",
            "alice",
            "bob",
        ]);
        assert_eq!(
            names(&entries_for(&all, None)),
            vec![
                ("_design", true),
                ("_local", true),
                ("alice.json", false),
                ("bob.json", false),
            ],
            "each prefix contributes exactly one folder, however many documents share it"
        );
    }

    #[test]
    fn inside_a_prefix_folder_only_that_prefixs_documents_are_listed() {
        let all = ids(&["_design/views", "_design/auth", "_local/checkpoint", "alice"]);
        assert_eq!(
            names(&entries_for(&all, Some("_design"))),
            vec![("views.json", false), ("auth.json", false)]
        );
    }

    #[test]
    fn a_query_value_is_encoded_so_json_survives_the_url() {
        assert_eq!(encode_query("\"alice\""), "%22alice%22");
        assert_eq!(encode_query("a b"), "a%20b");
        assert_eq!(encode_query("plain-id_1.0~"), "plain-id_1.0~");
    }
}
