//! The HTTP/JSON plumbing shared by the read-only document stores.
//!
//! `couchdb`, `elasticsearch`, `meilisearch`, `qdrant` and `solr` differ
//! entirely in what they ask for and how they page, and not at all in how
//! they ask: an authenticated request against `scheme://host:port`, a body
//! read under a cap, a status turned into something a person can act on, and
//! a document id carried through a file name without the path model eating a
//! separator.
//!
//! This module was deliberately **not** written when there were two such
//! stores - `CLAUDE.md` holds that three similar call sites are fine as three
//! call sites, and the `elasticsearch` work recorded "revisit at a third
//! HTTP/JSON store" rather than abstracting early. There are five now, so
//! the duplication is real rather than anticipated.
//!
//! What is deliberately *not* here: anything about what a store's JSON means.
//! Listing, paging and the shape of a document stay in each store's own
//! module, because that is where they genuinely differ.

use super::webdav::{agent, base64_padded, status_error};
use serde_json::Value;
use ureq::http::Request;

/// Most a single response body may be.
const MAX_BODY: u64 = 64 * 1024 * 1024;

/// How a store proves who it is.
///
/// Each of these is one store's documented scheme rather than a choice made
/// here: CouchDB and Solr take HTTP Basic, Meilisearch a bearer token,
/// Qdrant its own `api-key` header.
pub enum Auth {
    /// `Authorization: Basic base64(user:secret)`. A store with no
    /// credential configured uses this with both fields empty, which sends
    /// no header at all - so there is no separate "none" variant.
    Basic { user: String, secret: String },
    /// `Authorization: Bearer <secret>`.
    Bearer(String),
    /// A header the store defines itself, e.g. `api-key`.
    Header { name: &'static str, value: String },
}

impl Auth {
    /// Nothing to send when there is no secret *and* no user - an
    /// unauthenticated development server is the common case for every store
    /// here, and sending an empty credential is worse than sending none.
    fn header(&self) -> Option<(&'static str, String)> {
        match self {
            Auth::Basic { user, secret } if user.is_empty() && secret.is_empty() => None,
            Auth::Basic { user, secret } => Some((
                "Authorization",
                format!("Basic {}", base64_padded(format!("{user}:{secret}").as_bytes())),
            )),
            Auth::Bearer(secret) if secret.is_empty() => None,
            Auth::Bearer(secret) => Some(("Authorization", format!("Bearer {secret}"))),
            Auth::Header { value, .. } if value.is_empty() => None,
            Auth::Header { name, value } => Some((*name, value.clone())),
        }
    }
}

/// An authenticated JSON endpoint.
pub struct JsonHttp {
    agent: ureq::Agent,
    /// `scheme://host:port`, no trailing slash.
    base: String,
    auth: Option<(String, String)>,
    /// Pulls a store's own explanation out of an error body, when it has one
    /// more useful than the status. Elasticsearch's `error.reason` is the
    /// motivating case.
    detail: fn(&Value) -> Option<String>,
}

fn no_detail(_: &Value) -> Option<String> {
    None
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl JsonHttp {
    /// `scheme` follows the connection's own HTTPS flag.
    pub fn new(host: &str, port: u16, https: bool, auth: Auth) -> Result<Self, String> {
        let host = host.trim();
        if host.is_empty() {
            return Err("No server name".into());
        }
        let scheme = if https { "https" } else { "http" };
        Ok(JsonHttp {
            agent: agent(),
            base: format!("{scheme}://{host}:{port}"),
            auth: auth
                .header()
                .map(|(name, value)| (name.to_string(), value)),
            detail: no_detail,
        })
    }

    /// Reads a store-specific explanation out of error bodies.
    pub fn with_detail(mut self, detail: fn(&Value) -> Option<String>) -> Self {
        self.detail = detail;
        self
    }

    /// Sends one request and returns its body as text.
    ///
    /// `what` names the thing being fetched, for the error message - "the
    /// index list", "the document". A JSON body sets `Content-Type` for you.
    pub fn send(
        &self,
        method: &str,
        path_and_query: &str,
        body: Option<String>,
        what: &str,
    ) -> Result<String, String> {
        let mut builder = Request::builder()
            .method(method)
            .uri(format!("{}{path_and_query}", self.base));
        if let Some((name, value)) = &self.auth {
            builder = builder.header(name, value);
        }
        if body.is_some() {
            builder = builder.header("Content-Type", "application/json");
        }
        let request = builder.body(body.unwrap_or_default()).map_err(err)?;
        let response = self.agent.run(request).map_err(err)?;
        let status = response.status().as_u16();
        let text = response
            .into_body()
            .with_config()
            .limit(MAX_BODY)
            .read_to_string()
            .map_err(err)?;
        if status >= 400 {
            let reason = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|value| (self.detail)(&value));
            return Err(match reason {
                Some(reason) => format!("{} ({reason})", status_error(status, what)),
                None => status_error(status, what),
            });
        }
        Ok(text)
    }

    pub fn get(&self, path_and_query: &str, what: &str) -> Result<String, String> {
        self.send("GET", path_and_query, None, what)
    }

    /// `get`, parsed. The parse error names `what` too, since a store
    /// answering with something unexpected is a different problem from it
    /// refusing.
    pub fn get_json(&self, path_and_query: &str, what: &str) -> Result<Value, String> {
        parse(&self.get(path_and_query, what)?, what)
    }

    pub fn post_json(
        &self,
        path_and_query: &str,
        body: Value,
        what: &str,
    ) -> Result<Value, String> {
        parse(
            &self.send("POST", path_and_query, Some(body.to_string()), what)?,
            what,
        )
    }
}

pub fn parse(text: &str, what: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|e| format!("{what} could not be read ({e}).",))
}

// --- Documents as files -------------------------------------------------

/// A document id as a file name, with `.json` so the preview pane and Quick
/// Look recognise it - both dispatch on extension.
///
/// Only the three characters that would break the path model (or the
/// escaping itself) are encoded, so ordinary ids stay readable. **Escaped,
/// not sanitised**: several of these stores allow a `/` in an id, and a path
/// here is split on `/`, so sanitising would invent a folder and then fail to
/// fetch the document back.
pub fn file_name_for(id: &str) -> String {
    let mut out = String::with_capacity(id.len() + 5);
    for ch in id.chars() {
        match ch {
            '%' => out.push_str("%25"),
            '/' => out.push_str("%2F"),
            '\\' => out.push_str("%5C"),
            other => out.push(other),
        }
    }
    out.push_str(".json");
    out
}

/// The inverse: strip the extension this added, then undo the escapes.
pub fn id_from_file_name(name: &str) -> String {
    let body = name.strip_suffix(".json").unwrap_or(name);
    let bytes = body.as_bytes();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&body[i + 1..i + 3], 16)
        {
            out.push(byte as char);
            i += 3;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// A JSON value as the bytes opening it should produce.
pub fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// What a document takes as JSON, which is what opening it produces - so the
/// Size column agrees with the file that comes out.
pub fn json_size(value: &Value) -> u64 {
    value.to_string().len() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_id_survives_the_round_trip_through_a_file_name() {
        for id in [
            "plain",
            "with space",
            "a/b",
            "back\\slash",
            "already%2Fencoded",
            "_design/by_cuisine",
            "100%",
        ] {
            let name = file_name_for(id);
            assert!(name.ends_with(".json"), "{name} should carry the extension");
            assert!(
                !name.trim_end_matches(".json").contains(['/', '\\']),
                "{name} would invent a folder"
            );
            assert_eq!(id_from_file_name(&name), id, "round trip of {id}");
        }
    }

    /// A document genuinely called `notes.json` must not lose its own
    /// extension - exactly one `.json` is this module's.
    #[test]
    fn only_the_extension_this_module_added_is_stripped() {
        assert_eq!(file_name_for("notes.json"), "notes.json.json");
        assert_eq!(id_from_file_name("notes.json.json"), "notes.json");
    }

    #[test]
    fn an_empty_credential_sends_no_header() {
        assert!(
            Auth::Basic {
                user: String::new(),
                secret: String::new()
            }
            .header()
            .is_none()
        );
        assert!(Auth::Bearer(String::new()).header().is_none());
        assert!(
            Auth::Header {
                name: "api-key",
                value: String::new()
            }
            .header()
            .is_none()
        );
    }

    #[test]
    fn each_scheme_sends_the_header_its_store_documents() {
        let basic = Auth::Basic {
            user: "me".into(),
            secret: "pw".into(),
        }
        .header()
        .expect("a header");
        assert_eq!(basic.0, "Authorization");
        // base64("me:pw")
        assert_eq!(basic.1, "Basic bWU6cHc=");

        assert_eq!(
            Auth::Bearer("k".into()).header().expect("a header"),
            ("Authorization", "Bearer k".to_string())
        );
        assert_eq!(
            Auth::Header {
                name: "api-key",
                value: "k".into()
            }
            .header()
            .expect("a header"),
            ("api-key", "k".to_string())
        );
    }

    /// A user with no password still has to be sent - several of these
    /// stores accept a bare username.
    #[test]
    fn a_user_without_a_password_is_still_sent() {
        assert!(
            Auth::Basic {
                user: "me".into(),
                secret: String::new()
            }
            .header()
            .is_some()
        );
    }

    #[test]
    fn a_blank_host_is_refused_before_any_request() {
        assert!(JsonHttp::new("   ", 7700, false, Auth::Bearer(String::new())).is_err());
    }

    #[test]
    fn the_base_url_follows_the_https_flag() {
        let plain = JsonHttp::new("example.test", 7700, false, Auth::Bearer(String::new())).expect("built");
        assert_eq!(plain.base, "http://example.test:7700");
        let secure = JsonHttp::new("example.test", 443, true, Auth::Bearer(String::new())).expect("built");
        assert_eq!(secure.base, "https://example.test:443");
    }
}
