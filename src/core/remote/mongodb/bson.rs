//! Just enough BSON to send MongoDB a command and read what comes back.
//!
//! Reading produces `serde_json::Value` in MongoDB's own **relaxed extended
//! JSON** - `{"$oid": "..."}`, `{"$date": "..."}` and so on - because that is
//! what `mongoexport` writes and what anyone who has looked at a MongoDB
//! document will recognise. Writing only has to build command documents, so
//! it covers the handful of types those use rather than the whole format.

use serde_json::{Map, Value, json};

/// Nesting depth past which a document is refused rather than followed.
/// BSON is length-prefixed and a malformed (or hostile) document can claim
/// to nest for ever; this is read from a socket, so it is untrusted input.
const MAX_DEPTH: usize = 64;

pub type Document = Map<String, Value>;

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn err<T>(message: impl Into<String>) -> Result<T, Error> {
    Err(Error(message.into()))
}

// --- Writing -----------------------------------------------------------

/// Builds a BSON document for a command.
#[derive(Default)]
pub struct Builder {
    /// Element bytes, without the document's own length prefix or terminator.
    body: Vec<u8>,
}

impl Builder {
    pub fn new() -> Self {
        Builder::default()
    }

    fn key(&mut self, tag: u8, name: &str) {
        self.body.push(tag);
        self.body.extend_from_slice(name.as_bytes());
        self.body.push(0);
    }

    pub fn string(mut self, name: &str, value: &str) -> Self {
        self.key(0x02, name);
        self.body
            .extend_from_slice(&((value.len() + 1) as i32).to_le_bytes());
        self.body.extend_from_slice(value.as_bytes());
        self.body.push(0);
        self
    }

    pub fn int32(mut self, name: &str, value: i32) -> Self {
        self.key(0x10, name);
        self.body.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn int64(mut self, name: &str, value: i64) -> Self {
        self.key(0x12, name);
        self.body.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn bool(mut self, name: &str, value: bool) -> Self {
        self.key(0x08, name);
        self.body.push(u8::from(value));
        self
    }

    /// Generic binary (subtype 0x00), which is what SASL payloads use.
    pub fn binary(mut self, name: &str, value: &[u8]) -> Self {
        self.key(0x05, name);
        self.body
            .extend_from_slice(&(value.len() as i32).to_le_bytes());
        self.body.push(0x00);
        self.body.extend_from_slice(value);
        self
    }

    /// Only the tests need this: the stand-in server builds BSON from
    /// arbitrary JSON, so it nests documents, while no command this client
    /// actually sends does. `#[cfg(test)]` rather than deletion because the
    /// moment a command needs a nested document - a `find` filter, a sort -
    /// this is what it reaches for.
    #[cfg(test)]
    pub fn document(mut self, name: &str, value: Builder) -> Self {
        self.key(0x03, name);
        self.body.extend_from_slice(&value.finish());
        self
    }

    /// An array is a document whose keys are "0", "1", ... .
    pub fn array_of_int64(mut self, name: &str, values: &[i64]) -> Self {
        self.key(0x04, name);
        let mut inner = Builder::new();
        for (index, value) in values.iter().enumerate() {
            inner = inner.int64(&index.to_string(), *value);
        }
        self.body.extend_from_slice(&inner.finish());
        self
    }

    /// An array whose elements have already been built as a document with
    /// "0", "1", ... as its keys.
    ///
    /// Test-only for the same reason as `document` above - the commands sent
    /// here only ever need `array_of_int64`.
    #[cfg(test)]
    pub fn array_raw(mut self, name: &str, value: Builder) -> Self {
        self.key(0x04, name);
        self.body.extend_from_slice(&value.finish());
        self
    }

    /// The finished document: length, elements, terminator.
    pub fn finish(self) -> Vec<u8> {
        let total = self.body.len() + 5;
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(&(total as i32).to_le_bytes());
        out.extend_from_slice(&self.body);
        out.push(0);
        out
    }
}

// --- Reading -----------------------------------------------------------

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if self.at + n > self.bytes.len() {
            return err("the document ended early");
        }
        let slice = &self.bytes[self.at..self.at + n];
        self.at += n;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn i32(&mut self) -> Result<i32, Error> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i64(&mut self) -> Result<i64, Error> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn f64(&mut self) -> Result<f64, Error> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    /// A NUL-terminated key.
    fn cstring(&mut self) -> Result<String, Error> {
        let start = self.at;
        while self.at < self.bytes.len() && self.bytes[self.at] != 0 {
            self.at += 1;
        }
        if self.at >= self.bytes.len() {
            return err("an unterminated name");
        }
        let text = String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned();
        self.at += 1;
        Ok(text)
    }

    /// A length-prefixed, NUL-terminated string.
    fn string(&mut self) -> Result<String, Error> {
        let len = self.i32()?;
        if len < 1 {
            return err("a negative string length");
        }
        let bytes = self.take(len as usize)?;
        Ok(String::from_utf8_lossy(&bytes[..bytes.len() - 1]).into_owned())
    }

    fn document(&mut self, depth: usize) -> Result<Document, Error> {
        if depth > MAX_DEPTH {
            return err("the document nests too deeply");
        }
        let start = self.at;
        let len = self.i32()?;
        if len < 5 {
            return err("a malformed document length");
        }
        let end = start + len as usize;
        if end > self.bytes.len() {
            return err("a document longer than the message holding it");
        }

        let mut out = Map::new();
        loop {
            let tag = self.u8()?;
            if tag == 0 {
                break;
            }
            let name = self.cstring()?;
            let value = self.value(tag, depth)?;
            out.insert(name, value);
        }
        // Trust the length prefix over the element walk, so one unknown type
        // can't desynchronise everything after it.
        self.at = end;
        Ok(out)
    }

    fn array(&mut self, depth: usize) -> Result<Vec<Value>, Error> {
        // An array is a document with "0", "1", ... as its keys, and those
        // are already in order on the wire.
        Ok(self.document(depth)?.into_iter().map(|(_, v)| v).collect())
    }

    fn value(&mut self, tag: u8, depth: usize) -> Result<Value, Error> {
        Ok(match tag {
            0x01 => json!(self.f64()?),
            0x02 => Value::String(self.string()?),
            0x03 => Value::Object(self.document(depth + 1)?),
            0x04 => Value::Array(self.array(depth + 1)?),
            0x05 => {
                let len = self.i32()?;
                if len < 0 {
                    return err("a negative binary length");
                }
                let subtype = self.u8()?;
                let bytes = self.take(len as usize)?;
                json!({"$binary": {"base64": super::base64(bytes), "subType": format!("{subtype:02x}")}})
            }
            0x06 => Value::Null, // undefined (deprecated)
            0x07 => {
                let bytes = self.take(12)?;
                json!({ "$oid": hex(bytes) })
            }
            0x08 => Value::Bool(self.u8()? != 0),
            0x09 => {
                let millis = self.i64()?;
                json!({ "$date": iso8601(millis) })
            }
            0x0A => Value::Null,
            0x0B => {
                let pattern = self.cstring()?;
                let options = self.cstring()?;
                json!({"$regularExpression": {"pattern": pattern, "options": options}})
            }
            0x0C => {
                // DBPointer (deprecated): a namespace then an ObjectId.
                let namespace = self.string()?;
                let id = hex(self.take(12)?);
                json!({"$dbPointer": {"$ref": namespace, "$id": {"$oid": id}}})
            }
            0x0D => Value::String(self.string()?), // JavaScript
            0x0E => Value::String(self.string()?), // Symbol (deprecated)
            0x0F => {
                // Code with scope: a total length, the code, then a document.
                let _total = self.i32()?;
                let code = self.string()?;
                let scope = self.document(depth + 1)?;
                json!({"$code": code, "$scope": scope})
            }
            0x10 => json!(self.i32()?),
            0x11 => {
                // Timestamp: increment then seconds, both u32.
                let increment = self.i32()? as u32;
                let seconds = self.i32()? as u32;
                json!({"$timestamp": {"t": seconds, "i": increment}})
            }
            0x12 => json!(self.i64()?),
            0x13 => {
                // Decimal128. Decoding IEEE 754-2008 decimal properly is a
                // piece of work in its own right and these are rare, so the
                // raw bytes are reported rather than guessed at - visible and
                // honest, instead of a wrong number.
                let bytes = self.take(16)?;
                json!({ "$numberDecimalRaw": hex(bytes) })
            }
            0x7F => json!({ "$maxKey": 1 }),
            0xFF => json!({ "$minKey": 1 }),
            other => return err(format!("an unknown BSON type ({other:#04x})")),
        })
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Milliseconds since the epoch as ISO-8601, which is what relaxed extended
/// JSON uses for dates inside the range it can represent.
fn iso8601(millis: i64) -> String {
    chrono::DateTime::from_timestamp_millis(millis)
        .map(|t| t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
        .unwrap_or_else(|| millis.to_string())
}

/// Reads one document from the start of `bytes`.
pub fn read(bytes: &[u8]) -> Result<Document, Error> {
    Reader { bytes, at: 0 }.document(0)
}

/// The `$date` of a document's `_id`, when that id is an ObjectId.
///
/// An ObjectId's first four bytes are its creation time in seconds, so a
/// MongoDB document usually *does* have a usable timestamp - unlike a Redis
/// key or a CouchDB document, which have none at all.
pub fn object_id_time(value: &Value) -> Option<i64> {
    let oid = value.get("$oid")?.as_str()?;
    if oid.len() != 24 {
        return None;
    }
    i64::from_str_radix(&oid[..8], 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_document_round_trips_through_its_own_reader() {
        let bytes = Builder::new()
            .int32("find", 1)
            .string("$db", "shop")
            .int64("batchSize", 100)
            .bool("singleBatch", false)
            .finish();

        let doc = read(&bytes).expect("read");
        assert_eq!(doc["find"], json!(1));
        assert_eq!(doc["$db"], json!("shop"));
        assert_eq!(doc["batchSize"], json!(100));
        assert_eq!(doc["singleBatch"], json!(false));
        // The length prefix must match what was actually written.
        assert_eq!(
            i32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize,
            bytes.len()
        );
    }

    #[test]
    fn a_nested_document_and_an_array_round_trip() {
        let inner = Builder::new().string("mechanism", "SCRAM-SHA-256");
        let bytes = Builder::new()
            .document("options", inner)
            .array_of_int64("cursors", &[7, 8])
            .finish();

        let doc = read(&bytes).expect("read");
        assert_eq!(doc["options"]["mechanism"], json!("SCRAM-SHA-256"));
        assert_eq!(doc["cursors"], json!([7, 8]));
    }

    #[test]
    fn binary_survives_as_the_bytes_it_went_in_as() {
        let payload = b"n,,n=admin,r=rOprNGfwEbeRWgbNEkqO".to_vec();
        let bytes = Builder::new().binary("payload", &payload).finish();
        let doc = read(&bytes).expect("read");
        let base64 = doc["payload"]["$binary"]["base64"].as_str().expect("base64");
        assert_eq!(super::super::base64(&payload), base64);
        assert_eq!(doc["payload"]["$binary"]["subType"], json!("00"));
    }

    #[test]
    fn an_object_id_reads_as_extended_json_and_carries_its_time() {
        // 0x5f0e1b2c seconds since the epoch, then 8 bytes of counter.
        let mut raw = Vec::new();
        raw.push(0x07u8);
        raw.extend_from_slice(b"_id\0");
        raw.extend_from_slice(&[
            0x5f, 0x0e, 0x1b, 0x2c, 1, 2, 3, 4, 5, 6, 7, 8,
        ]);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&((raw.len() + 5) as i32).to_le_bytes());
        bytes.extend_from_slice(&raw);
        bytes.push(0);

        let doc = read(&bytes).expect("read");
        assert_eq!(doc["_id"]["$oid"], json!("5f0e1b2c0102030405060708"));
        assert_eq!(object_id_time(&doc["_id"]), Some(0x5f0e_1b2c));
        // Anything that isn't an ObjectId simply has no time.
        assert_eq!(object_id_time(&json!("plain-string-id")), None);
        assert_eq!(object_id_time(&json!({"$oid": "too-short"})), None);
    }

    #[test]
    fn a_date_reads_as_iso8601() {
        let mut raw = vec![0x09u8];
        raw.extend_from_slice(b"when\0");
        raw.extend_from_slice(&1_600_000_000_123i64.to_le_bytes());
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&((raw.len() + 5) as i32).to_le_bytes());
        bytes.extend_from_slice(&raw);
        bytes.push(0);

        let doc = read(&bytes).expect("read");
        assert_eq!(doc["when"]["$date"], json!("2020-09-13T12:26:40.123Z"));
    }

    #[test]
    fn a_truncated_document_is_an_error_rather_than_a_panic() {
        // Every prefix of a valid document is malformed; none may panic,
        // because these bytes come off a socket.
        let full = Builder::new()
            .string("hello", "world")
            .int32("n", 1)
            .finish();
        for cut in 1..full.len() {
            let _ = read(&full[..cut]);
        }
        assert!(read(&[]).is_err());
        assert!(read(&[0, 0, 0, 0]).is_err(), "a zero length is malformed");
    }

    #[test]
    fn a_document_claiming_absurd_nesting_is_refused() {
        // A hostile server could otherwise drive the reader into the stack.
        let mut bytes = Vec::new();
        for _ in 0..(MAX_DEPTH + 10) {
            let mut inner = vec![0x03u8];
            inner.extend_from_slice(b"a\0");
            inner.extend_from_slice(&bytes);
            bytes = inner;
        }
        let mut doc = Vec::new();
        doc.extend_from_slice(&((bytes.len() + 5) as i32).to_le_bytes());
        doc.extend_from_slice(&bytes);
        doc.push(0);
        assert!(read(&doc).is_err());
    }
}
