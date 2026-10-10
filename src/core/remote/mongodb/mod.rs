//! MongoDB, browsed like a folder tree. **Read-only.**
//!
//! ```text
//! /                          every database      (listDatabases)
//! /shop                      its collections     (listCollections)
//! /shop/orders               its documents       (find)
//! /shop/orders/<id>.json     one document
//! ```
//!
//! A document store maps onto folders better than a SQL database does - a
//! document really is a file - and MongoDB fits best of all, because an
//! `ObjectId` carries the time it was made, so the Modified column has real
//! values here where Redis and CouchDB leave it blank.
//!
//! The protocol is hand-rolled, like `redis`'s RESP2 and unlike `sqlite`'s
//! bundled library. The distinction worth keeping: SQLite had a failure mode
//! where data lives *outside* the file being read (WAL), so a hand-written
//! reader would have been silently wrong. MongoDB has no such mode - the
//! server reports what it has - so the risk here is auth and framing, which
//! fail loudly.
//!
//! **Nothing here writes.** Same reasoning as `redis` and `couchdb`: a file
//! manager's reflexes are fast and destructive, and a live database has no
//! Recycle Bin.

mod bson;
mod scram;

use super::{RemoteConnection, RemoteEntry, RemoteFs};
use bson::Builder;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Documents asked for in one batch.
const BATCH: i32 = 1_000;

/// Stop listing a collection after this many documents.
const MAX_DOCS: usize = 20_000;

/// Longest single message accepted from the server. MongoDB's own limit is
/// 48 MB; this is the guard against a length prefix that claims more.
const MAX_MESSAGE: usize = 64 * 1024 * 1024;

const OP_MSG: i32 = 2013;

pub struct MongoFs {
    stream: TcpStream,
    request_id: i32,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn read_only() -> String {
    "This MongoDB location is read-only.".to_string()
}

/// `alice` → `alice.json`, so the preview pane recognises the content. Same
/// rule as `couchdb`, including stripping exactly one trailing `.json` back.
fn file_name_for(id: &str) -> String {
    format!("{id}.json")
}

fn id_from_file_name(name: &str) -> String {
    name.strip_suffix(".json").unwrap_or(name).to_string()
}

/// How a document's `_id` is shown as a file name. An ObjectId becomes its
/// hex, anything else its JSON - a string id without its quotes, so the
/// common case reads naturally.
fn id_label(id: &Value) -> String {
    if let Some(oid) = id.get("$oid").and_then(|v| v.as_str()) {
        return oid.to_string();
    }
    match id {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
    // Separators would break the path model, so they are replaced rather
    // than allowed to invent folders.
    .replace(['/', '\\'], "_")
}

pub(super) fn base64(bytes: &[u8]) -> String {
    super::webdav::base64_padded(bytes)
}

pub(super) fn unbase64(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes() {
        if byte == b'=' {
            break;
        }
        let value = ALPHABET.iter().position(|c| *c == byte)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

impl MongoFs {
    pub fn connect(conn: &RemoteConnection, secret: &str) -> Result<Self, String> {
        if conn.host.trim().is_empty() {
            return Err("No server name".into());
        }
        let stream = super::sftp::tcp(&conn.host, conn.port)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(err)?;
        let mut fs = MongoFs {
            stream,
            request_id: 1,
        };

        // Fails here rather than on the first listing, so a wrong port or a
        // server that isn't MongoDB is reported while the user is still
        // looking at the connection they just set up.
        let hello = fs.command("admin", Builder::new().int32("hello", 1))?;
        if !hello.contains_key("maxWireVersion") && !hello.contains_key("ismaster") {
            return Err("That server answered, but it isn't MongoDB.".into());
        }

        if !conn.username.trim().is_empty() {
            // Credentials live in a database of their own - `admin` unless
            // the user says otherwise, which is where MongoDB puts them by
            // default.
            let source = if conn.path.trim().is_empty() {
                "admin".to_string()
            } else {
                conn.path.trim().trim_matches('/').to_string()
            };
            fs.sign_in(conn.username.trim(), secret, &source)?;
        }
        Ok(fs)
    }

    fn sign_in(&mut self, user: &str, password: &str, source: &str) -> Result<(), String> {
        let mut scram = scram::Scram::new(user, password, scram::random_nonce());

        let start = self.command(
            source,
            Builder::new()
                .int32("saslStart", 1)
                .string("mechanism", "SCRAM-SHA-256")
                .binary("payload", &scram.client_first()),
        )?;
        let conversation = start
            .get("conversationId")
            .and_then(|v| v.as_i64())
            .ok_or("the server didn't start a sign-in")?;
        let challenge = payload_of(&start).ok_or("the server sent no challenge")?;

        let answer = scram.client_final(&challenge)?;
        let next = self.command(
            source,
            Builder::new()
                .int32("saslContinue", 1)
                .int32("conversationId", conversation as i32)
                .binary("payload", &answer),
        )?;
        let server_final = payload_of(&next).ok_or("the server sent no signature")?;
        scram.verify_server(&server_final)?;

        // One more round trip when the server hasn't finished yet, which is
        // how it signals it has nothing further to say.
        if next.get("done").and_then(|v| v.as_bool()) != Some(true) {
            self.command(
                source,
                Builder::new()
                    .int32("saslContinue", 1)
                    .int32("conversationId", conversation as i32)
                    .binary("payload", &[]),
            )?;
        }
        Ok(())
    }

    /// Sends one command to `database` and returns its reply.
    fn command(&mut self, database: &str, body: Builder) -> Result<bson::Document, String> {
        let document = body.string("$db", database).finish();

        let mut message = Vec::with_capacity(document.len() + 21);
        let length = 16 + 4 + 1 + document.len();
        message.extend_from_slice(&(length as i32).to_le_bytes());
        message.extend_from_slice(&self.request_id.to_le_bytes());
        message.extend_from_slice(&0i32.to_le_bytes()); // responseTo
        message.extend_from_slice(&OP_MSG.to_le_bytes());
        message.extend_from_slice(&0u32.to_le_bytes()); // flagBits
        message.push(0); // section kind 0: the body
        message.extend_from_slice(&document);
        self.request_id = self.request_id.wrapping_add(1);

        self.stream.write_all(&message).map_err(err)?;
        self.stream.flush().map_err(err)?;

        let reply = self.read_message()?;
        // The reply is a header, flags, a section kind, then the document.
        if reply.len() < 5 {
            return Err("the server sent a message with no body".into());
        }
        let document = bson::read(&reply[5..]).map_err(err)?;

        if document.get("ok").and_then(as_number) != Some(1.0) {
            let message = document
                .get("errmsg")
                .and_then(|v| v.as_str())
                .unwrap_or("the server refused the command");
            return Err(explain(message, document.get("code").and_then(|c| c.as_i64())));
        }
        Ok(document)
    }

    /// Reads one framed message, returning everything after the 16-byte
    /// header.
    fn read_message(&mut self) -> Result<Vec<u8>, String> {
        let mut header = [0u8; 16];
        self.stream.read_exact(&mut header).map_err(err)?;
        let length = i32::from_le_bytes(header[..4].try_into().unwrap());
        if !(16..=MAX_MESSAGE as i32).contains(&length) {
            return Err(format!("the server announced a {length}-byte message"));
        }
        let op_code = i32::from_le_bytes(header[12..16].try_into().unwrap());
        if op_code != OP_MSG {
            return Err(format!("the server replied with opcode {op_code}, not OP_MSG"));
        }
        let mut body = vec![0u8; length as usize - 16];
        self.stream.read_exact(&mut body).map_err(err)?;
        Ok(body)
    }

    fn databases(&mut self) -> Result<Vec<String>, String> {
        let reply = self.command("admin", Builder::new().int32("listDatabases", 1))?;
        Ok(reply
            .get("databases")
            .and_then(|v| v.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|entry| Some(entry.get("name")?.as_str()?.to_string()))
                    .collect()
            })
            .unwrap_or_default())
    }

    fn collections(&mut self, database: &str) -> Result<Vec<String>, String> {
        let reply = self.command(
            database,
            Builder::new()
                .int32("listCollections", 1)
                .bool("nameOnly", true),
        )?;
        let mut names: Vec<String> = cursor_batch(&reply, "firstBatch")
            .iter()
            .filter_map(|entry| Some(entry.get("name")?.as_str()?.to_string()))
            .collect();
        names.sort();
        Ok(names)
    }

    /// Every document in a collection, up to `MAX_DOCS`, following the cursor
    /// with `getMore` until it is exhausted.
    fn documents(&mut self, database: &str, collection: &str) -> Result<Vec<Value>, String> {
        let reply = self.command(
            database,
            Builder::new()
                .string("find", collection)
                .int32("batchSize", BATCH),
        )?;
        let mut out: Vec<Value> = cursor_batch(&reply, "firstBatch").to_vec();
        let mut cursor = cursor_id(&reply);

        while cursor != 0 && out.len() < MAX_DOCS {
            let reply = self.command(
                database,
                Builder::new()
                    .int64("getMore", cursor)
                    .string("collection", collection)
                    .int32("batchSize", BATCH),
            )?;
            let batch = cursor_batch(&reply, "nextBatch");
            if batch.is_empty() {
                break;
            }
            out.extend(batch.iter().cloned());
            cursor = cursor_id(&reply);
        }

        // Leaving a cursor open holds resources on the server until it times
        // out; this is read-only but not inconsiderate.
        if cursor != 0 {
            let _ = self.command(
                database,
                Builder::new()
                    .string("killCursors", collection)
                    .array_of_int64("cursors", &[cursor]),
            );
        }

        out.truncate(MAX_DOCS);
        Ok(out)
    }

    fn document(
        &mut self,
        database: &str,
        collection: &str,
        id: &str,
    ) -> Result<Value, String> {
        // Matched client-side rather than with a filter: an `_id` can be an
        // ObjectId, a string, a number or a document, and building a filter
        // for each would mean guessing which from the text in a file name.
        for document in self.documents(database, collection)? {
            if let Some(found) = document.get("_id")
                && id_label(found) == id
            {
                return Ok(document);
            }
        }
        Err("That document is no longer there.".into())
    }
}

fn as_number(value: &Value) -> Option<f64> {
    value.as_f64()
}

fn payload_of(reply: &bson::Document) -> Option<Vec<u8>> {
    let base64 = reply.get("payload")?.get("$binary")?.get("base64")?.as_str()?;
    unbase64(base64)
}

fn cursor_batch<'a>(reply: &'a bson::Document, which: &str) -> &'a [Value] {
    reply
        .get("cursor")
        .and_then(|cursor| cursor.get(which))
        .and_then(|batch| batch.as_array())
        .map(|batch| batch.as_slice())
        .unwrap_or(&[])
}

fn cursor_id(reply: &bson::Document) -> i64 {
    reply
        .get("cursor")
        .and_then(|cursor| cursor.get("id"))
        .and_then(|id| id.as_i64())
        .unwrap_or(0)
}

/// Turns the server's own error into something a person can act on.
fn explain(message: &str, code: Option<i64>) -> String {
    match code {
        // AuthenticationFailed / Unauthorized.
        Some(18) => "Signing in failed (wrong user name or password).".into(),
        Some(13) => format!("Access denied ({message})."),
        _ => message.to_string(),
    }
}

/// `/shop/orders/alice.json` → the database, collection and document it names.
fn split(path: &str) -> (Option<String>, Option<String>, Option<String>) {
    let segments = super::segments_of(path);
    let mut parts = segments.into_iter();
    (
        parts.next(),
        parts.next(),
        parts.next().as_deref().map(id_from_file_name),
    )
}

impl RemoteFs for MongoFs {
    fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>, String> {
        let folder = |name: String| RemoteEntry {
            name,
            is_dir: true,
            size: None,
            modified: None,
        };

        match split(dir) {
            (None, _, _) => Ok(self.databases()?.into_iter().map(folder).collect()),
            (Some(database), None, _) => {
                Ok(self.collections(&database)?.into_iter().map(folder).collect())
            }
            (Some(database), Some(collection), None) => {
                let documents = self.documents(&database, &collection)?;
                Ok(documents
                    .iter()
                    .filter_map(|document| {
                        let id = document.get("_id")?;
                        Some(RemoteEntry {
                            name: file_name_for(&id_label(id)),
                            is_dir: false,
                            // What the document takes as JSON, which is what
                            // opening it produces - not its size on the
                            // server, which the server doesn't report here.
                            size: Some(document.to_string().len() as u64),
                            // An ObjectId knows when it was made; any other
                            // kind of id doesn't, and leaves this blank.
                            modified: bson::object_id_time(id),
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
        let (Some(database), Some(collection), Some(id)) = split(file) else {
            return Err("Not a document.".into());
        };
        let document = self.document(&database, &collection, &id)?;
        let pretty = serde_json::to_string_pretty(&document).map_err(err)?;
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
    use serde_json::json;


    /// A stand-in MongoDB: OP_MSG framing, a real SCRAM-SHA-256 handshake,
    /// and a cursor that needs following.
    ///
    /// Same approach as `redis` and `couchdb`, and the only way to exercise
    /// the parts that aren't reachable from a unit test - the message
    /// framing, the sign-in as a conversation rather than a formula, and the
    /// `getMore` loop. The framing is written out by hand here rather than
    /// reusing the client's own, so a mistake in it can't cancel itself out.
    mod fake {
        use super::super::{bson, scram};
        use serde_json::json;
        use std::io::{Read, Write};
        use std::net::{TcpListener, TcpStream};

        const SALT: &[u8] = b"0123456789abcdef";
        const ITERATIONS: u32 = 4096;

        /// Starts the stand-in on a free port. `docs` is the whole of
        /// `shop.orders`; `batch` is how many it returns at a time, so a test
        /// can force the cursor loop to go round.
        pub fn start(password: &'static str, docs: Vec<serde_json::Value>, batch: usize) -> u16 {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().expect("addr").port();
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { break };
                    let docs = docs.clone();
                    std::thread::spawn(move || serve(&mut stream, password, &docs, batch));
                }
            });
            port
        }

        fn serve(stream: &mut TcpStream, password: &str, docs: &[serde_json::Value], batch: usize) {
            let mut auth: Option<(String, String)> = None; // (client-first-bare, server-first)
            let mut cursor: usize = 0;
            loop {
                let Some(command) = read_command(stream) else { return };
                let reply = answer(&command, password, docs, batch, &mut auth, &mut cursor);
                if write_reply(stream, reply).is_err() {
                    return;
                }
            }
        }

        fn read_command(stream: &mut TcpStream) -> Option<bson::Document> {
            let mut header = [0u8; 16];
            stream.read_exact(&mut header).ok()?;
            let length = i32::from_le_bytes(header[..4].try_into().unwrap()) as usize;
            assert_eq!(
                i32::from_le_bytes(header[12..16].try_into().unwrap()),
                2013,
                "the client must send OP_MSG"
            );
            let mut body = vec![0u8; length - 16];
            stream.read_exact(&mut body).ok()?;
            // flags(4) then the section kind byte, then the document.
            assert_eq!(body[4], 0, "section kind 0 is the body");
            bson::read(&body[5..]).ok()
        }

        fn write_reply(stream: &mut TcpStream, document: Vec<u8>) -> std::io::Result<()> {
            let length = 16 + 4 + 1 + document.len();
            let mut out = Vec::with_capacity(length);
            out.extend_from_slice(&(length as i32).to_le_bytes());
            out.extend_from_slice(&7i32.to_le_bytes()); // requestID
            out.extend_from_slice(&1i32.to_le_bytes()); // responseTo
            out.extend_from_slice(&2013i32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.push(0);
            out.extend_from_slice(&document);
            stream.write_all(&out)?;
            stream.flush()
        }

        fn answer(
            command: &bson::Document,
            password: &str,
            docs: &[serde_json::Value],
            batch: usize,
            auth: &mut Option<(String, String)>,
            cursor: &mut usize,
        ) -> Vec<u8> {
            if command.contains_key("hello") {
                return bson::Builder::new()
                    .int32("maxWireVersion", 17)
                    .bool("ok_placeholder", true)
                    .int32("ok", 1)
                    .finish();
            }
            if command.contains_key("saslStart") {
                let first = payload(command);
                let bare = first.splitn(3, ',').nth(2).unwrap_or("").to_string();
                let client_nonce = bare
                    .split(',')
                    .find_map(|p| p.strip_prefix("r="))
                    .unwrap_or("")
                    .to_string();
                let server_first = format!(
                    "r={client_nonce}serverpart,s={},i={ITERATIONS}",
                    super::super::base64(SALT)
                );
                *auth = Some((bare, server_first.clone()));
                return bson::Builder::new()
                    .int32("conversationId", 1)
                    .binary("payload", server_first.as_bytes())
                    .bool("done", false)
                    .int32("ok", 1)
                    .finish();
            }
            if command.contains_key("saslContinue") {
                let client_final = payload(command);
                let (bare, server_first) = auth.clone().expect("saslStart first");
                let without_proof = client_final
                    .rsplit_once(",p=")
                    .map(|(left, _)| left.to_string())
                    .unwrap_or_default();
                let auth_message = format!("{bare},{server_first},{without_proof}");
                let salted = scram::pbkdf2(password.as_bytes(), SALT, ITERATIONS);
                let server_key = scram::hmac(&salted, b"Server Key");
                let signature = scram::hmac(&server_key, auth_message.as_bytes());
                let server_final = format!("v={}", super::super::base64(&signature));
                return bson::Builder::new()
                    .int32("conversationId", 1)
                    .binary("payload", server_final.as_bytes())
                    .bool("done", true)
                    .int32("ok", 1)
                    .finish();
            }
            if command.contains_key("listDatabases") {
                return document_from(json!({
                    "databases": [{"name": "shop"}, {"name": "admin"}],
                    "ok": 1
                }));
            }
            if command.contains_key("listCollections") {
                return document_from(json!({
                    "cursor": {"id": 0i64, "firstBatch": [{"name": "orders"}, {"name": "people"}]},
                    "ok": 1
                }));
            }
            if command.contains_key("find") {
                *cursor = 0;
                let taken: Vec<serde_json::Value> = docs.iter().take(batch).cloned().collect();
                *cursor = taken.len();
                let id = if *cursor < docs.len() { 77i64 } else { 0 };
                return document_from(json!({
                    "cursor": {"id": id, "firstBatch": taken},
                    "ok": 1
                }));
            }
            if command.contains_key("getMore") {
                let taken: Vec<serde_json::Value> =
                    docs.iter().skip(*cursor).take(batch).cloned().collect();
                *cursor += taken.len();
                let id = if *cursor < docs.len() { 77i64 } else { 0 };
                return document_from(json!({
                    "cursor": {"id": id, "nextBatch": taken},
                    "ok": 1
                }));
            }
            if command.contains_key("killCursors") {
                return document_from(json!({"ok": 1}));
            }
            document_from(json!({"ok": 0, "errmsg": "unknown command"}))
        }

        fn payload(command: &bson::Document) -> String {
            let base64 = command["payload"]["$binary"]["base64"].as_str().expect("payload");
            String::from_utf8(super::super::unbase64(base64).expect("base64")).expect("utf8")
        }

        /// Turns a JSON reply into BSON. Only the shapes this stand-in sends.
        fn document_from(value: serde_json::Value) -> Vec<u8> {
            fn build(map: &serde_json::Map<String, serde_json::Value>) -> bson::Builder {
                let mut out = bson::Builder::new();
                for (key, value) in map {
                    out = match value {
                        serde_json::Value::String(text) => out.string(key, text),
                        serde_json::Value::Bool(flag) => out.bool(key, *flag),
                        serde_json::Value::Object(inner) => out.document(key, build(inner)),
                        serde_json::Value::Array(items) => {
                            let mut array = bson::Builder::new();
                            for (index, item) in items.iter().enumerate() {
                                let name = index.to_string();
                                array = match item {
                                    serde_json::Value::Object(inner) => {
                                        array.document(&name, build(inner))
                                    }
                                    serde_json::Value::String(text) => array.string(&name, text),
                                    other => array.string(&name, &other.to_string()),
                                };
                            }
                            out.array_raw(key, array)
                        }
                        serde_json::Value::Number(number) if number.is_i64() => {
                            let n = number.as_i64().unwrap();
                            if (i32::MIN as i64..=i32::MAX as i64).contains(&n) && key != "id" {
                                out.int32(key, n as i32)
                            } else {
                                out.int64(key, n)
                            }
                        }
                        other => out.string(key, &other.to_string()),
                    };
                }
                out
            }
            build(value.as_object().expect("an object")).finish()
        }
    }

    fn connection(port: u16, user: &str) -> RemoteConnection {
        RemoteConnection {
            id: 1,
            kind: RemoteKind::MongoDb,
            host: "127.0.0.1".into(),
            port,
            username: user.into(),
            ..Default::default()
        }
    }

    fn names(entries: &[RemoteEntry]) -> Vec<(&str, bool)> {
        entries.iter().map(|e| (e.name.as_str(), e.is_dir)).collect()
    }

    fn document(id: &str) -> serde_json::Value {
        json!({"_id": {"$oid": id}, "total": 42})
    }

    #[test]
    fn a_real_client_signs_in_and_walks_the_tree() {
        let docs = vec![
            document("6724ab00000000000000001f"),
            document("6724ab000000000000000020"),
        ];
        let port = fake::start("pencil", docs, 10);
        // A real SCRAM-SHA-256 exchange: a fresh random nonce, a real
        // PBKDF2, and the client checking the server's own signature.
        let mut fs = MongoFs::connect(&connection(port, "admin"), "pencil").expect("connect");

        assert_eq!(
            names(&fs.list("/").expect("databases")),
            vec![("shop", true), ("admin", true)]
        );
        assert_eq!(
            names(&fs.list("/shop").expect("collections")),
            vec![("orders", true), ("people", true)]
        );

        let documents = fs.list("/shop/orders").expect("documents");
        assert_eq!(
            names(&documents),
            vec![
                ("6724ab00000000000000001f.json", false),
                ("6724ab000000000000000020.json", false),
            ]
        );
        // The ObjectId's own timestamp - the thing Redis and CouchDB can't
        // give the Modified column.
        assert_eq!(documents[0].modified, Some(0x6724_ab00));
        assert!(documents[0].size.unwrap_or(0) > 0);
    }

    #[test]
    fn a_wrong_password_is_refused_by_the_signature_check() {
        let port = fake::start("pencil", vec![document("6724ab00000000000000001f")], 10);
        // The stand-in signs with the real password, so a client using the
        // wrong one computes a different expectation and must reject it -
        // this is the check that stops a server being believed on its word.
        let error = match MongoFs::connect(&connection(port, "admin"), "not-pencil") {
            Err(error) => error,
            Ok(_) => panic!("a wrong password should have been refused"),
        };
        assert!(
            error.contains("signature"),
            "unhelpful error: {error}"
        );
    }

    #[test]
    fn a_cursor_is_followed_until_it_is_exhausted() {
        // Five documents, two at a time: one find plus two getMores.
        let docs: Vec<serde_json::Value> = (0..5)
            .map(|n| document(&format!("6724ab00000000000000{n:04}")))
            .collect();
        let port = fake::start("pencil", docs, 2);
        let mut fs = MongoFs::connect(&connection(port, "admin"), "pencil").expect("connect");

        let documents = fs.list("/shop/orders").expect("documents");
        assert_eq!(documents.len(), 5, "every batch, each document once");
        let unique: std::collections::HashSet<&str> =
            documents.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(unique.len(), 5, "a document was listed twice");
    }

    #[test]
    fn a_server_without_a_sign_in_still_works() {
        // Plenty of development servers run with no auth at all; an empty
        // user name must skip SCRAM rather than fail.
        let port = fake::start("pencil", vec![document("6724ab00000000000000001f")], 10);
        let mut fs = MongoFs::connect(&connection(port, ""), "").expect("connect");
        assert_eq!(fs.list("/").expect("databases").len(), 2);
    }

    #[test]
    fn opening_a_document_returns_it_pretty_printed() {
        let port = fake::start("pencil", vec![document("6724ab00000000000000001f")], 10);
        let mut fs = MongoFs::connect(&connection(port, "admin"), "pencil").expect("connect");

        let mut out = Vec::new();
        let mut size = 0u64;
        fs.download(
            "/shop/orders/6724ab00000000000000001f.json",
            &mut out,
            &mut |n| size = n,
        )
        .expect("download");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("\"$oid\": \"6724ab00000000000000001f\""), "got: {text}");
        assert!(text.contains('\n'), "should be pretty-printed: {text}");
        assert_eq!(size, text.len() as u64);
    }

    #[test]
    fn every_write_is_refused() {
        let port = fake::start("pencil", vec![document("6724ab00000000000000001f")], 10);
        let mut fs = MongoFs::connect(&connection(port, "admin"), "pencil").expect("connect");
        let mut empty = std::io::empty();
        assert!(fs.upload(&mut empty, 0, "/shop/orders/x.json", &mut |_| {}).is_err());
        assert!(fs.mkdir("/shop2").is_err());
        assert!(fs.rename("/a/b/c.json", "/a/b/d.json").is_err());
        assert!(fs.remove_file("/shop/orders/x.json").is_err());
        assert!(fs.remove_dir("/shop").is_err());
    }

    #[test]
    fn a_path_splits_into_database_collection_and_document() {
        assert_eq!(split("/"), (None, None, None));
        assert_eq!(split("/shop"), (Some("shop".into()), None, None));
        assert_eq!(
            split("/shop/orders"),
            (Some("shop".into()), Some("orders".into()), None)
        );
        assert_eq!(
            split("/shop/orders/abc123.json"),
            (
                Some("shop".into()),
                Some("orders".into()),
                Some("abc123".into())
            )
        );
    }

    #[test]
    fn a_document_id_becomes_a_file_name_whatever_kind_of_id_it_is() {
        assert_eq!(id_label(&json!({"$oid": "6724ab00000000000000001f"})), "6724ab00000000000000001f");
        assert_eq!(id_label(&json!("alice")), "alice");
        assert_eq!(id_label(&json!(42)), "42");
        // A separator in an id would otherwise invent a folder level.
        assert_eq!(id_label(&json!("a/b")), "a_b");
        assert_eq!(id_label(&json!("a\\b")), "a_b");
    }

    #[test]
    fn a_file_name_round_trips_back_to_its_id() {
        assert_eq!(id_from_file_name(&file_name_for("alice")), "alice");
        // An id that already ends in `.json` keeps it.
        assert_eq!(id_from_file_name(&file_name_for("notes.json")), "notes.json");
    }

    #[test]
    fn base64_round_trips_including_every_padding_length() {
        for n in 0..8 {
            let bytes: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            let encoded = base64(&bytes);
            assert_eq!(unbase64(&encoded).as_deref(), Some(bytes.as_slice()), "n={n}");
        }
        // Known vectors, to catch an alphabet that is merely self-consistent.
        assert_eq!(base64(b"pencil"), "cGVuY2ls");
        assert_eq!(unbase64("cGVuY2ls").as_deref(), Some(&b"pencil"[..]));
        assert_eq!(unbase64("not base64!"), None);
    }

    #[test]
    fn a_cursor_reply_is_read_and_an_exhausted_one_reports_zero() {
        let reply: bson::Document = serde_json::from_value(json!({
            "cursor": {"id": 42, "firstBatch": [{"_id": "a"}, {"_id": "b"}]},
            "ok": 1.0
        }))
        .expect("document");
        assert_eq!(cursor_batch(&reply, "firstBatch").len(), 2);
        assert_eq!(cursor_id(&reply), 42);

        let done: bson::Document =
            serde_json::from_value(json!({"cursor": {"id": 0, "nextBatch": []}, "ok": 1.0}))
                .expect("document");
        assert_eq!(cursor_id(&done), 0);
        assert!(cursor_batch(&done, "nextBatch").is_empty());

        // A reply with no cursor at all must not panic.
        let empty: bson::Document = serde_json::from_value(json!({"ok": 1.0})).expect("document");
        assert_eq!(cursor_id(&empty), 0);
        assert!(cursor_batch(&empty, "firstBatch").is_empty());
    }

    #[test]
    fn a_sign_in_failure_is_explained_rather_than_echoed() {
        assert!(explain("whatever the server said", Some(18)).contains("wrong user name"));
        assert!(explain("not authorized on shop", Some(13)).contains("Access denied"));
        assert_eq!(explain("something else", None), "something else");
    }
}
