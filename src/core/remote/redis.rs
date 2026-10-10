//! Redis, browsed like a folder tree. **Read-only.**
//!
//! Redis is a key-value store, not a filesystem, so this is a deliberate
//! mapping rather than a protocol implementation:
//!
//! * Keys are split on `:`, the near-universal Redis naming convention, so
//!   `user:1000:name` browses as `user` → `1000` → `name`. Those "folders"
//!   are a naming habit, not something the server knows about - exactly the
//!   same trick `s3` already plays with key prefixes.
//! * Only a `string` key is really "a file with bytes". Every other type
//!   (hash, list, set, zset, stream) is rendered as JSON for reading.
//! * Redis has no modification time, so `modified` is always `None` and the
//!   Modified/Created columns stay blank. `TTL` is expiry, not modification,
//!   and putting it there would be a lie.
//!
//! **Nothing here writes.** A file manager's reflexes are fast and
//! destructive - select-all then Delete is muscle memory - and against a
//! live database, with no Recycle Bin and no undo, that is a different
//! severity of accident from deleting a file. Every mutating method returns
//! an error saying so. If writing is ever added it needs its own
//! confirmation, not the ordinary file gestures.
//!
//! The protocol is RESP2, hand-rolled rather than pulling in a client crate,
//! matching how `network`/`network_mdns`/`network_wsd` already hand-roll
//! NBSTAT, mDNS and WS-Discovery here.

use super::{RemoteConnection, RemoteEntry, RemoteFs};
use std::io::{BufReader, BufWriter, Read, Write};
use std::net::TcpStream;

/// Keys are split on this. Redis itself attaches no meaning to it.
const SEP: char = ':';
/// Keys read in one `SCAN` round trip. Larger means fewer round trips and a
/// longer block on the server, which is the trade `COUNT` exists for.
const SCAN_COUNT: usize = 500;
/// Stop listing a folder after this many keys. A keyspace can hold millions,
/// and a file list that never finishes is worse than an incomplete one.
const MAX_KEYS: usize = 20_000;
/// Longest value rendered when a key is opened.
const MAX_VALUE: usize = 8 * 1024 * 1024;
/// Elements read from a collection before it is truncated.
const MAX_ELEMENTS: usize = 10_000;

/// A RESP2 reply.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Simple(String),
    Int(i64),
    Bulk(Vec<u8>),
    Nil,
    Array(Vec<Value>),
}

impl Value {
    fn as_text(&self) -> String {
        match self {
            Value::Simple(s) => s.clone(),
            Value::Int(n) => n.to_string(),
            Value::Bulk(b) => String::from_utf8_lossy(b).into_owned(),
            Value::Nil => String::new(),
            Value::Array(_) => String::new(),
        }
    }

    fn as_array(&self) -> &[Value] {
        match self {
            Value::Array(items) => items,
            _ => &[],
        }
    }
}

pub struct RedisFs {
    reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Escapes the glob metacharacters `SCAN ... MATCH` understands, so a key
/// prefix containing one is matched literally rather than as a pattern.
fn escape_glob(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(ch, '*' | '?' | '[' | ']' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// The key a path refers to: `/user/1000/name` → `user:1000:name`.
fn key_of(path: &str) -> String {
    super::segments_of(path).join(&SEP.to_string())
}

/// The prefix a folder's children share: `/user` → `user:`, root → ``.
fn prefix_of(dir: &str) -> String {
    let segments = super::segments_of(dir);
    if segments.is_empty() {
        String::new()
    } else {
        format!("{}{SEP}", segments.join(&SEP.to_string()))
    }
}

impl RedisFs {
    pub fn connect(conn: &RemoteConnection, secret: &str) -> Result<Self, String> {
        let stream = super::sftp::tcp(&conn.host, conn.port)?;
        let reader = BufReader::new(stream.try_clone().map_err(err)?);
        let writer = BufWriter::new(stream);
        let mut fs = RedisFs { reader, writer };

        // AUTH is optional, and takes the user name too on Redis 6+ ACLs.
        if !secret.is_empty() {
            let reply = if conn.username.trim().is_empty() {
                fs.command(&["AUTH", secret])?
            } else {
                fs.command(&["AUTH", conn.username.trim(), secret])?
            };
            let _ = reply;
        }
        if conn.database > 0 {
            fs.command(&["SELECT", &conn.database.to_string()])?;
        }
        // Fail here rather than on the first listing, so a wrong port or a
        // server that is not Redis is reported while the user is still
        // looking at the connection they just set up.
        fs.command(&["PING"])?;
        Ok(fs)
    }

    /// Sends one command and reads its reply.
    fn command(&mut self, args: &[&str]) -> Result<Value, String> {
        self.send(args)?;
        self.writer.flush().map_err(err)?;
        self.read_reply()
    }

    /// Queues a command without waiting for its reply, so a batch can be
    /// sent in one round trip instead of one each (see `types_and_sizes`).
    fn send(&mut self, args: &[&str]) -> Result<(), String> {
        write!(self.writer, "*{}\r\n", args.len()).map_err(err)?;
        for arg in args {
            write!(self.writer, "${}\r\n", arg.len()).map_err(err)?;
            self.writer.write_all(arg.as_bytes()).map_err(err)?;
            self.writer.write_all(b"\r\n").map_err(err)?;
        }
        Ok(())
    }

    fn read_line(&mut self) -> Result<String, String> {
        let mut line = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let read = self.reader.read(&mut byte).map_err(err)?;
            if read == 0 {
                return Err("the server closed the connection".into());
            }
            if byte[0] == b'\n' {
                break;
            }
            if byte[0] != b'\r' {
                line.push(byte[0]);
            }
        }
        Ok(String::from_utf8_lossy(&line).into_owned())
    }

    fn read_reply(&mut self) -> Result<Value, String> {
        let line = self.read_line()?;
        let (tag, rest) = line.split_at(line.char_indices().nth(1).map_or(line.len(), |(i, _)| i));
        match tag {
            "+" => Ok(Value::Simple(rest.to_string())),
            "-" => Err(rest.to_string()),
            ":" => Ok(Value::Int(rest.parse().unwrap_or(0))),
            "$" => {
                let len: i64 = rest.parse().map_err(|_| "a malformed bulk length")?;
                if len < 0 {
                    return Ok(Value::Nil);
                }
                let mut buf = vec![0u8; len as usize];
                self.reader.read_exact(&mut buf).map_err(err)?;
                let mut crlf = [0u8; 2];
                self.reader.read_exact(&mut crlf).map_err(err)?;
                Ok(Value::Bulk(buf))
            }
            "*" => {
                let len: i64 = rest.parse().map_err(|_| "a malformed array length")?;
                if len < 0 {
                    return Ok(Value::Nil);
                }
                let mut items = Vec::with_capacity(len.min(MAX_ELEMENTS as i64) as usize);
                for _ in 0..len {
                    items.push(self.read_reply()?);
                }
                Ok(Value::Array(items))
            }
            other => Err(format!("the server answered with an unexpected {other:?}")),
        }
    }

    /// Every key under `pattern`, following `SCAN`'s cursor.
    ///
    /// `SCAN` rather than `KEYS`: `KEYS *` blocks the server for the whole
    /// sweep, which on a real instance is an outage. The trade is that this
    /// is not a snapshot - a key added or removed mid-scan may or may not
    /// appear - which for a folder listing is fine.
    fn scan(&mut self, pattern: &str) -> Result<Vec<String>, String> {
        let mut keys = Vec::new();
        let mut cursor = "0".to_string();
        loop {
            let reply = self.command(&[
                "SCAN",
                &cursor,
                "MATCH",
                pattern,
                "COUNT",
                &SCAN_COUNT.to_string(),
            ])?;
            let parts = reply.as_array();
            if parts.len() != 2 {
                return Err("SCAN answered in a shape this does not understand".into());
            }
            cursor = parts[0].as_text();
            for key in parts[1].as_array() {
                keys.push(key.as_text());
            }
            if cursor == "0" || keys.len() >= MAX_KEYS {
                break;
            }
        }
        keys.truncate(MAX_KEYS);
        Ok(keys)
    }

    /// `TYPE` (and `STRLEN` for strings) for every key, pipelined.
    ///
    /// One round trip for the batch rather than one per key: a folder with a
    /// thousand leaf keys would otherwise be a thousand sequential waits,
    /// which over any real network is the difference between instant and
    /// unusable.
    fn types_and_sizes(&mut self, keys: &[String]) -> Result<Vec<(String, Option<u64>)>, String> {
        for key in keys {
            self.send(&["TYPE", key])?;
        }
        self.writer.flush().map_err(err)?;
        let mut types = Vec::with_capacity(keys.len());
        for _ in keys {
            types.push(self.read_reply()?.as_text());
        }

        let strings: Vec<usize> = types
            .iter()
            .enumerate()
            .filter(|(_, t)| *t == "string")
            .map(|(i, _)| i)
            .collect();
        for &i in &strings {
            self.send(&["STRLEN", &keys[i]])?;
        }
        self.writer.flush().map_err(err)?;
        let mut sizes = vec![None; keys.len()];
        for &i in &strings {
            if let Value::Int(n) = self.read_reply()? {
                sizes[i] = Some(n.max(0) as u64);
            }
        }
        Ok(types.into_iter().zip(sizes).collect())
    }

    /// A key's value, rendered for reading. Strings come back as they are;
    /// everything else becomes JSON, because a hash or a stream has no byte
    /// form to show.
    fn render(&mut self, key: &str) -> Result<Vec<u8>, String> {
        let kind = self.command(&["TYPE", key])?.as_text();
        let json = |v: serde_json::Value| -> Result<Vec<u8>, String> {
            serde_json::to_vec_pretty(&v).map_err(err)
        };
        match kind.as_str() {
            "string" => match self.command(&["GET", key])? {
                Value::Bulk(mut bytes) => {
                    bytes.truncate(MAX_VALUE);
                    Ok(bytes)
                }
                Value::Nil => Err("that key no longer exists".into()),
                other => Ok(other.as_text().into_bytes()),
            },
            "list" => {
                let items = self.command(&["LRANGE", key, "0", &(MAX_ELEMENTS - 1).to_string()])?;
                json(serde_json::Value::Array(
                    items.as_array().iter().map(text_json).collect(),
                ))
            }
            "set" => {
                let items = self.command(&["SMEMBERS", key])?;
                json(serde_json::Value::Array(
                    items.as_array().iter().map(text_json).collect(),
                ))
            }
            "hash" => {
                let items = self.command(&["HGETALL", key])?;
                let mut map = serde_json::Map::new();
                for pair in items.as_array().chunks(2) {
                    if let [field, value] = pair {
                        map.insert(field.as_text(), text_json(value));
                    }
                }
                json(serde_json::Value::Object(map))
            }
            "zset" => {
                let items = self.command(&[
                    "ZRANGE",
                    key,
                    "0",
                    &(MAX_ELEMENTS - 1).to_string(),
                    "WITHSCORES",
                ])?;
                let mut map = serde_json::Map::new();
                for pair in items.as_array().chunks(2) {
                    if let [member, score] = pair {
                        map.insert(member.as_text(), text_json(score));
                    }
                }
                json(serde_json::Value::Object(map))
            }
            "stream" => {
                let items =
                    self.command(&["XRANGE", key, "-", "+", "COUNT", &MAX_ELEMENTS.to_string()])?;
                json(serde_json::Value::Array(
                    items.as_array().iter().map(stream_entry_json).collect(),
                ))
            }
            "none" => Err("that key no longer exists".into()),
            other => Err(format!("`{other}` keys cannot be shown yet")),
        }
    }
}

fn text_json(v: &Value) -> serde_json::Value {
    serde_json::Value::String(v.as_text())
}

/// One `XRANGE` entry: `[id, [field, value, ...]]`.
fn stream_entry_json(entry: &Value) -> serde_json::Value {
    let parts = entry.as_array();
    let id = parts.first().map(|v| v.as_text()).unwrap_or_default();
    let mut fields = serde_json::Map::new();
    if let Some(pairs) = parts.get(1) {
        for pair in pairs.as_array().chunks(2) {
            if let [field, value] = pair {
                fields.insert(field.as_text(), text_json(value));
            }
        }
    }
    let mut out = serde_json::Map::new();
    out.insert("id".into(), serde_json::Value::String(id));
    out.insert("fields".into(), serde_json::Value::Object(fields));
    serde_json::Value::Object(out)
}

/// Everything a read-only location refuses, in the same words.
fn read_only() -> String {
    "This Redis location is read-only.".to_string()
}

impl RemoteFs for RedisFs {
    fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>, String> {
        let prefix = prefix_of(dir);
        let keys = self.scan(&format!("{}*", escape_glob(&prefix)))?;

        // Split each key's remainder at the first separator: anything before
        // one is a folder, anything without one is a key in this folder.
        let mut folders: Vec<String> = Vec::new();
        let mut leaves: Vec<String> = Vec::new();
        for key in &keys {
            let Some(rest) = key.strip_prefix(prefix.as_str()) else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            match rest.split_once(SEP) {
                Some((folder, _)) => {
                    if !folders.iter().any(|f| f == folder) {
                        folders.push(folder.to_string());
                    }
                }
                None => leaves.push(key.clone()),
            }
        }

        let typed = self.types_and_sizes(&leaves)?;
        let mut entries: Vec<RemoteEntry> = folders
            .into_iter()
            .map(|name| RemoteEntry {
                name,
                is_dir: true,
                size: None,
                modified: None,
            })
            .collect();
        for (key, (_kind, size)) in leaves.iter().zip(typed) {
            entries.push(RemoteEntry {
                name: key.strip_prefix(prefix.as_str()).unwrap_or(key).to_string(),
                is_dir: false,
                size,
                modified: None,
            });
        }
        Ok(entries)
    }

    fn download(
        &mut self,
        file: &str,
        to: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), String> {
        let bytes = self.render(&key_of(file))?;
        to.write_all(&bytes).map_err(err)?;
        progress(bytes.len() as u64);
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
        // There is nothing to create: a "folder" here is just a shared
        // prefix in other keys' names.
        Err(read_only())
    }

    fn rename(&mut self, _from: &str, _to: &str) -> Result<(), String> {
        Err(read_only())
    }

    fn remove_file(&mut self, _file: &str) -> Result<(), String> {
        Err(read_only())
    }

    fn remove_dir(&mut self, _dir: &str) -> Result<(), String> {
        // Deleting "an empty folder" would mean deleting every key sharing a
        // prefix, which is not the same operation at all.
        Err(read_only())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_becomes_a_colon_separated_key() {
        assert_eq!(key_of("/user/1000/name"), "user:1000:name");
        assert_eq!(key_of("/single"), "single");
        assert_eq!(key_of("/"), "");
    }

    #[test]
    fn a_folder_is_a_key_prefix() {
        assert_eq!(prefix_of("/"), "");
        assert_eq!(prefix_of("/user"), "user:");
        assert_eq!(prefix_of("/user/1000"), "user:1000:");
    }

    #[test]
    fn glob_metacharacters_in_a_prefix_are_matched_literally() {
        // A key really named `cache[v1]:x` must not be read as a character
        // class by SCAN's MATCH, or the folder lists nothing at all.
        assert_eq!(escape_glob("cache[v1]:"), r"cache\[v1\]:");
        assert_eq!(escape_glob("a*b?c"), r"a\*b\?c");
        assert_eq!(escape_glob("plain:"), "plain:");
    }

    #[test]
    fn every_mutating_operation_is_refused() {
        // The whole point of the prototype: a file manager's ordinary
        // gestures must not reach a live database.
        let message = read_only();
        assert!(message.contains("read-only"));
    }

    /// A stand-in Redis speaking just enough RESP2 to drive a real
    /// `RedisFs` over a real socket.
    ///
    /// Worth more than pointing the tests at an installed Redis would be:
    /// it runs anywhere, needs nothing installed, and pins the exact wire
    /// shapes this client depends on - a `SCAN` reply's cursor-plus-array,
    /// and replies arriving in the order a pipelined batch was sent.
    mod fake {
        use std::io::{BufRead, BufReader, Write};
        use std::net::{TcpListener, TcpStream};

        /// Reads one RESP array-of-bulk-strings command.
        fn read_command(reader: &mut BufReader<&TcpStream>) -> Option<Vec<String>> {
            let mut line = String::new();
            if reader.read_line(&mut line).ok()? == 0 {
                return None;
            }
            let count: usize = line.trim().strip_prefix('*')?.parse().ok()?;
            let mut args = Vec::with_capacity(count);
            for _ in 0..count {
                let mut header = String::new();
                reader.read_line(&mut header).ok()?;
                let len: usize = header.trim().strip_prefix('$')?.parse().ok()?;
                let mut buf = vec![0u8; len + 2]; // value + CRLF
                std::io::Read::read_exact(reader, &mut buf).ok()?;
                buf.truncate(len);
                args.push(String::from_utf8_lossy(&buf).into_owned());
            }
            Some(args)
        }

        fn bulk(out: &mut TcpStream, s: &str) {
            let _ = write!(out, "${}\r\n{}\r\n", s.len(), s);
        }

        /// Starts the stand-in on an arbitrary free port and returns it.
        /// `keys` is the whole keyspace: (key, type, value).
        pub fn start(keys: Vec<(&'static str, &'static str, &'static str)>) -> u16 {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().expect("addr").port();
            std::thread::spawn(move || {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                let mut out = stream.try_clone().expect("clone");
                let mut reader = BufReader::new(&stream);
                while let Some(args) = read_command(&mut reader) {
                    let name = args[0].to_ascii_uppercase();
                    match name.as_str() {
                        "PING" => {
                            let _ = out.write_all(b"+PONG\r\n");
                        }
                        "AUTH" | "SELECT" => {
                            let _ = out.write_all(b"+OK\r\n");
                        }
                        "SCAN" => {
                            // MATCH is args[3]; only the literal prefix
                            // before the trailing `*` is honoured here,
                            // which is all this client ever sends.
                            let pattern = args.get(3).cloned().unwrap_or_default();
                            let prefix = pattern.trim_end_matches('*').replace('\\', "");
                            let hits: Vec<&str> = keys
                                .iter()
                                .map(|(k, _, _)| *k)
                                .filter(|k| k.starts_with(&prefix))
                                .collect();
                            // Cursor 0 = done, in one round trip.
                            let _ = write!(out, "*2\r\n$1\r\n0\r\n*{}\r\n", hits.len());
                            for k in hits {
                                bulk(&mut out, k);
                            }
                        }
                        "TYPE" => {
                            let kind = keys
                                .iter()
                                .find(|(k, _, _)| *k == args[1])
                                .map(|(_, t, _)| *t)
                                .unwrap_or("none");
                            let _ = write!(out, "+{kind}\r\n");
                        }
                        "STRLEN" => {
                            let len = keys
                                .iter()
                                .find(|(k, _, _)| *k == args[1])
                                .map(|(_, _, v)| v.len())
                                .unwrap_or(0);
                            let _ = write!(out, ":{len}\r\n");
                        }
                        "GET" => {
                            match keys.iter().find(|(k, _, _)| *k == args[1]) {
                                Some((_, _, v)) => bulk(&mut out, v),
                                None => {
                                    let _ = out.write_all(b"$-1\r\n");
                                }
                            }
                        }
                        "HGETALL" => {
                            // The value is stored as "field=value,field=value".
                            let raw = keys
                                .iter()
                                .find(|(k, _, _)| *k == args[1])
                                .map(|(_, _, v)| *v)
                                .unwrap_or("");
                            let pairs: Vec<&str> = raw.split(',').filter(|p| !p.is_empty()).collect();
                            let _ = write!(out, "*{}\r\n", pairs.len() * 2);
                            for pair in pairs {
                                let (f, v) = pair.split_once('=').unwrap_or((pair, ""));
                                bulk(&mut out, f);
                                bulk(&mut out, v);
                            }
                        }
                        _ => {
                            let _ = out.write_all(b"+OK\r\n");
                        }
                    }
                    let _ = out.flush();
                }
            });
            port
        }
    }

    fn connect_to(port: u16) -> RedisFs {
        let conn = RemoteConnection {
            kind: super::super::RemoteKind::Redis,
            host: "127.0.0.1".to_string(),
            port,
            ..Default::default()
        };
        RedisFs::connect(&conn, "").expect("the stand-in should accept a connection")
    }

    #[test]
    fn a_keyspace_browses_as_a_folder_tree() {
        let port = fake::start(vec![
            ("user:1000:name", "string", "Ada"),
            ("user:1000:email", "string", "ada@example.com"),
            ("user:1001:name", "string", "Grace"),
            ("session", "string", "abc"),
        ]);
        let mut fs = connect_to(port);

        // Root: one folder for the shared `user` prefix, and the key that
        // has no separator at all sits here as a file.
        let root = fs.list("/").expect("list root");
        let mut names: Vec<(String, bool)> =
            root.iter().map(|e| (e.name.clone(), e.is_dir)).collect();
        names.sort();
        assert_eq!(
            names,
            vec![("session".to_string(), false), ("user".to_string(), true)]
        );
        // A string key reports its length; nothing reports a modified time,
        // because Redis has none to give.
        let session = root.iter().find(|e| e.name == "session").unwrap();
        assert_eq!(session.size, Some(3));
        assert!(root.iter().all(|e| e.modified.is_none()));

        // One level down, the ids become folders.
        let users = fs.list("/user").expect("list /user");
        let mut ids: Vec<String> = users.iter().map(|e| e.name.clone()).collect();
        ids.sort();
        assert_eq!(ids, vec!["1000", "1001"]);
        assert!(users.iter().all(|e| e.is_dir));

        // And the leaves are the keys themselves.
        let fields = fs.list("/user/1000").expect("list /user/1000");
        let mut leaves: Vec<String> = fields.iter().map(|e| e.name.clone()).collect();
        leaves.sort();
        assert_eq!(leaves, vec!["email", "name"]);
        assert!(fields.iter().all(|e| !e.is_dir));
    }

    #[test]
    fn a_string_key_opens_as_its_own_bytes() {
        let port = fake::start(vec![("user:1000:name", "string", "Ada Lovelace")]);
        let mut fs = connect_to(port);
        let mut out = Vec::new();
        fs.download("/user/1000/name", &mut out, &mut |_| {})
            .expect("download");
        assert_eq!(String::from_utf8_lossy(&out), "Ada Lovelace");
    }

    #[test]
    fn a_hash_key_opens_as_json_because_it_has_no_byte_form() {
        let port = fake::start(vec![("cfg", "hash", "host=localhost,port=6379")]);
        let mut fs = connect_to(port);
        let mut out = Vec::new();
        fs.download("/cfg", &mut out, &mut |_| {}).expect("download");
        let parsed: serde_json::Value =
            serde_json::from_slice(&out).expect("a hash should render as valid JSON");
        assert_eq!(parsed["host"], "localhost");
        assert_eq!(parsed["port"], "6379");
    }

    #[test]
    fn writing_is_refused_rather_than_attempted() {
        let port = fake::start(vec![("k", "string", "v")]);
        let mut fs = connect_to(port);
        assert!(fs.mkdir("/new").is_err());
        assert!(fs.rename("/k", "/k2").is_err());
        assert!(fs.remove_file("/k").is_err());
        assert!(fs.remove_dir("/k").is_err());
        let mut empty = std::io::empty();
        assert!(fs.upload(&mut empty, 0, "/k", &mut |_| {}).is_err());
    }

    #[test]
    fn a_stream_entry_becomes_an_object_with_its_id_and_fields() {
        let entry = Value::Array(vec![
            Value::Bulk(b"1526919030474-55".to_vec()),
            Value::Array(vec![
                Value::Bulk(b"temperature".to_vec()),
                Value::Bulk(b"36".to_vec()),
            ]),
        ]);
        let json = stream_entry_json(&entry);
        assert_eq!(json["id"], "1526919030474-55");
        assert_eq!(json["fields"]["temperature"], "36");
    }

    /// Against a **real** server, not the stand-in above.
    ///
    /// `#[ignore]`d because it needs one running - the same shape as
    /// `webdav`'s TLS-handshake test. Run it with:
    ///
    /// ```text
    /// EDEN_TEST_REDIS=127.0.0.1:6379 cargo test -- --ignored real_redis
    /// ```
    ///
    /// This is how the "Valkey, KeyDB and Dragonfly work too" claim is checked:
    /// they all speak RESP2, so point this at one of them and it proves the
    /// claim rather than asserting it. Verified against Redis 8.6.3 and
    /// Valkey 7.2.13.
    #[test]
    #[ignore]
    fn a_real_redis_compatible_server_lists_its_keyspace() {
        let Ok(endpoint) = std::env::var("EDEN_TEST_REDIS") else {
            panic!("set EDEN_TEST_REDIS=host:port");
        };
        let (host, port) = endpoint.rsplit_once(':').expect("host:port");
        let conn = RemoteConnection {
            id: 900_001,
            kind: crate::core::remote::RemoteKind::Redis,
            host: host.to_string(),
            port: port.parse().expect("a port"),
            ..Default::default()
        };

        let mut fs = RedisFs::connect(&conn, "").expect("connect");
        let top = fs.list("/").expect("list the root");
        assert!(!top.is_empty(), "the keyspace looked empty");
        assert!(
            top.iter().any(|entry| entry.is_dir),
            "keys sharing a `:` prefix should have become folders: {:?}",
            top.iter().map(|e| &e.name).collect::<Vec<_>>()
        );

        // And one level down, a folder's own contents.
        let folder = top.iter().find(|e| e.is_dir).expect("a folder");
        let inside = fs
            .list(&format!("/{}", folder.name))
            .expect("list inside a folder");
        assert!(!inside.is_empty(), "{} looked empty", folder.name);

        // Read-only, against the real thing as well as the fake.
        assert!(fs.remove_file("/definitely/not/a/key").is_err());
    }
}
