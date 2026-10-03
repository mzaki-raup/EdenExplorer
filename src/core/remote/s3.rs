//! Amazon S3 and S3-compatible storage (MinIO, Backblaze B2, Wasabi,
//! Cloudflare R2, ...), signed with AWS Signature Version 4. A bucket is
//! shown as folders by splitting object keys on `/`; a new folder is an
//! empty `name/` object, and renaming copies then deletes.

use super::webdav::{agent, encode_path, status_error};
use super::{RemoteConnection, RemoteEntry, RemoteFs};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use ureq::http::Request;

pub struct S3Fs {
    agent: ureq::Agent,
    scheme: &'static str,
    /// `host[:port]` requests go to.
    host: String,
    /// Path prefix before the object key: `/bucket` (path style) or empty.
    bucket_path: String,
    bucket: String,
    region: String,
    access_key: String,
    secret: String,
}

/// Counts bytes read, reporting progress.
pub(super) struct Counting<'a> {
    pub inner: &'a mut dyn Read,
    pub done: u64,
    pub progress: &'a mut dyn FnMut(u64),
}

impl Read for Counting<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.done += n as u64;
        (self.progress)(self.done);
        Ok(n)
    }
}

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Encodes a query value (everything but unreserved characters).
fn encode_query(s: &str) -> String {
    encode_path(s).replace('/', "%2F")
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// An object key for a path (`/a/b` → `a/b`).
fn key_of(path: &str) -> String {
    path.trim_start_matches('/').to_string()
}

/// A folder's key prefix (`/a` → `a/`, `/` → ``).
fn prefix_of(dir: &str) -> String {
    let key = key_of(dir);
    if key.is_empty() {
        key
    } else {
        format!("{}/", key.trim_end_matches('/'))
    }
}

/// The Authorization header for a request (AWS Signature Version 4).
#[allow(clippy::too_many_arguments)]
fn authorization(
    method: &str,
    canonical_uri: &str,
    canonical_query: &str,
    headers: &[(String, String)],
    payload_hash: &str,
    amz_date: &str,
    region: &str,
    access_key: &str,
    secret: &str,
) -> String {
    let mut headers: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.trim().to_string()))
        .collect();
    headers.sort();
    let canonical_headers: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed_headers = headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let date = &amz_date[..8];
    let scope = format!("{date}/{region}/s3/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        hex(&Sha256::digest(canonical_request.as_bytes()))
    );
    let key = hmac(format!("AWS4{secret}").as_bytes(), date);
    let key = hmac(&key, region);
    let key = hmac(&key, "s3");
    let key = hmac(&key, "aws4_request");
    let signature = hex(&hmac(&key, &string_to_sign));
    format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
    )
}

impl S3Fs {
    pub fn new(conn: &RemoteConnection, secret: &str) -> Result<Self, String> {
        if conn.bucket.trim().is_empty() {
            return Err("No bucket name".into());
        }
        let region = if conn.region.trim().is_empty() {
            "us-east-1".to_string()
        } else {
            conn.region.trim().to_string()
        };
        let bucket = conn.bucket.trim().to_string();
        let custom = !conn.host.trim().is_empty();
        let default_port = if conn.https { 443 } else { 80 };
        let (host, bucket_path) = if custom {
            // Custom endpoints (MinIO, ...) use path-style addresses.
            let host = if conn.port == default_port || conn.port == 0 {
                conn.host.trim().to_string()
            } else {
                format!("{}:{}", conn.host.trim(), conn.port)
            };
            (host, format!("/{bucket}"))
        } else {
            (format!("{bucket}.s3.{region}.amazonaws.com"), String::new())
        };
        let mut fs = Self {
            agent: agent(),
            scheme: if conn.https || !custom {
                "https"
            } else {
                "http"
            },
            host,
            bucket_path,
            bucket,
            region,
            access_key: conn.username.trim().to_string(),
            secret: secret.to_string(),
        };
        fs.list("/")?;
        Ok(fs)
    }

    /// A signed request for `key` with `query` (already sorted, encoded).
    fn signed(
        &self,
        method: &str,
        key: &str,
        query: &str,
        payload_hash: &str,
        extra: &[(&str, String)],
    ) -> ureq::http::request::Builder {
        let uri_path = format!("{}/{}", self.bucket_path, encode_path(key));
        let amz_date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let mut headers: Vec<(String, String)> = vec![
            ("host".into(), self.host.clone()),
            ("x-amz-content-sha256".into(), payload_hash.into()),
            ("x-amz-date".into(), amz_date.clone()),
        ];
        headers.extend(extra.iter().map(|(k, v)| (k.to_string(), v.clone())));
        let auth = authorization(
            method,
            &uri_path,
            query,
            &headers,
            payload_hash,
            &amz_date,
            &self.region,
            &self.access_key,
            &self.secret,
        );
        let url = if query.is_empty() {
            format!("{}://{}{uri_path}", self.scheme, self.host)
        } else {
            format!("{}://{}{uri_path}?{query}", self.scheme, self.host)
        };
        let mut builder = Request::builder()
            .method(method)
            .uri(url)
            .header("Authorization", auth);
        for (k, v) in headers.iter().filter(|(k, _)| k != "host") {
            builder = builder.header(k.as_str(), v.as_str());
        }
        builder
    }

    fn send(
        &self,
        builder: ureq::http::request::Builder,
        what: &str,
    ) -> Result<ureq::http::Response<ureq::Body>, String> {
        let response = self
            .agent
            .run(builder.body(()).map_err(err)?)
            .map_err(err)?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            Ok(response)
        } else {
            let body = response.into_body().read_to_string().unwrap_or_default();
            let code = xml_values(&body, "Code")
                .into_iter()
                .next()
                .unwrap_or_default();
            Err(match code.as_str() {
                "InvalidAccessKeyId" | "SignatureDoesNotMatch" => {
                    "Signing in failed (wrong access key or secret).".into()
                }
                "NoSuchBucket" => format!("The bucket {} doesn't exist.", self.bucket),
                "" => status_error(status, what),
                other => format!(
                    "{} ({other}).",
                    status_error(status, what).trim_end_matches('.')
                ),
            })
        }
    }

    /// Every object key under `prefix` (all levels), for folder operations.
    fn all_keys(&self, prefix: &str) -> Result<Vec<String>, String> {
        let mut keys = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut query = vec![
                ("list-type".to_string(), "2".to_string()),
                ("prefix".to_string(), prefix.to_string()),
            ];
            if let Some(t) = &token {
                query.push(("continuation-token".into(), t.clone()));
            }
            query.sort();
            let q = query
                .iter()
                .map(|(k, v)| format!("{k}={}", encode_query(v)))
                .collect::<Vec<_>>()
                .join("&");
            let body = self
                .send(self.signed("GET", "", &q, EMPTY_SHA256, &[]), "listing")?
                .into_body()
                .read_to_string()
                .map_err(err)?;
            keys.extend(xml_values(&body, "Key"));
            token = xml_values(&body, "NextContinuationToken")
                .into_iter()
                .next();
            if token.is_none() {
                return Ok(keys);
            }
        }
    }

    fn copy_object(&self, from_key: &str, to_key: &str) -> Result<(), String> {
        let source = format!("/{}/{}", self.bucket, encode_path(from_key));
        self.send(
            self.signed(
                "PUT",
                to_key,
                "",
                EMPTY_SHA256,
                &[("x-amz-copy-source", source)],
            ),
            "copying",
        )?;
        Ok(())
    }

    fn delete_key(&self, key: &str) -> Result<(), String> {
        self.send(
            self.signed("DELETE", key, "", EMPTY_SHA256, &[]),
            "deleting",
        )?;
        Ok(())
    }
}

/// The text of every `<tag>` element, in order (S3's XML is flat enough).
fn xml_values(xml: &str, tag: &str) -> Vec<String> {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(&close) else { break };
        out.push(unescape(&after[..end]));
        rest = &after[end + close.len()..];
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Entries of one `ListObjectsV2` page under `prefix`.
fn parse_list(xml: &str, prefix: &str) -> Vec<RemoteEntry> {
    let mut out = Vec::new();
    for block in xml.split("<Contents>").skip(1) {
        let block = block.split("</Contents>").next().unwrap_or("");
        let Some(key) = xml_values(block, "Key").into_iter().next() else {
            continue;
        };
        let name = key.strip_prefix(prefix).unwrap_or(&key).to_string();
        // The folder's own marker object.
        if name.is_empty() || name.contains('/') {
            continue;
        }
        out.push(RemoteEntry {
            name,
            is_dir: false,
            size: xml_values(block, "Size")
                .first()
                .and_then(|s| s.parse().ok()),
            modified: xml_values(block, "LastModified")
                .first()
                .and_then(|s| super::parse_date(s)),
        });
    }
    for block in xml.split("<CommonPrefixes>").skip(1) {
        let block = block.split("</CommonPrefixes>").next().unwrap_or("");
        if let Some(p) = xml_values(block, "Prefix").into_iter().next() {
            let name = p
                .strip_prefix(prefix)
                .unwrap_or(&p)
                .trim_end_matches('/')
                .to_string();
            if !name.is_empty() {
                out.push(RemoteEntry {
                    name,
                    is_dir: true,
                    size: None,
                    modified: None,
                });
            }
        }
    }
    out
}

impl RemoteFs for S3Fs {
    fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>, String> {
        let prefix = prefix_of(dir);
        let mut entries = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut query = vec![
                ("delimiter".to_string(), "/".to_string()),
                ("list-type".to_string(), "2".to_string()),
                ("prefix".to_string(), prefix.clone()),
            ];
            if let Some(t) = &token {
                query.push(("continuation-token".into(), t.clone()));
            }
            query.sort();
            let q = query
                .iter()
                .map(|(k, v)| format!("{k}={}", encode_query(v)))
                .collect::<Vec<_>>()
                .join("&");
            let body = self
                .send(self.signed("GET", "", &q, EMPTY_SHA256, &[]), "listing")?
                .into_body()
                .read_to_string()
                .map_err(err)?;
            entries.extend(parse_list(&body, &prefix));
            token = xml_values(&body, "NextContinuationToken")
                .into_iter()
                .next();
            if token.is_none() {
                return Ok(entries);
            }
        }
    }

    fn download(
        &mut self,
        file: &str,
        to: &mut dyn Write,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), String> {
        let response = self.send(
            self.signed("GET", &key_of(file), "", EMPTY_SHA256, &[]),
            "downloading",
        )?;
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
        let mut counting = Counting {
            inner: from,
            done: 0,
            progress,
        };
        let builder = self
            .signed("PUT", &key_of(file), "", "UNSIGNED-PAYLOAD", &[])
            .header("Content-Length", size.to_string());
        let response = self
            .agent
            .run(
                builder
                    .body(ureq::SendBody::from_reader(&mut counting))
                    .map_err(err)?,
            )
            .map_err(err)?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(status_error(status, "uploading"))
        }
    }

    fn mkdir(&mut self, dir: &str) -> Result<(), String> {
        let builder = self
            .signed("PUT", &prefix_of(dir), "", EMPTY_SHA256, &[])
            .header("Content-Length", "0");
        self.send(builder, "creating the folder").map(drop)
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        let folder_keys = self.all_keys(&prefix_of(from))?;
        if folder_keys.is_empty() {
            self.copy_object(&key_of(from), &key_of(to))?;
            return self.delete_key(&key_of(from));
        }
        // A folder: every object under it moves.
        let (old, new) = (prefix_of(from), prefix_of(to));
        for key in &folder_keys {
            let rest = key.strip_prefix(&old).unwrap_or(key);
            self.copy_object(key, &format!("{new}{rest}"))?;
        }
        for key in &folder_keys {
            self.delete_key(key)?;
        }
        Ok(())
    }

    fn remove_file(&mut self, file: &str) -> Result<(), String> {
        self.delete_key(&key_of(file))
    }

    fn remove_dir(&mut self, dir: &str) -> Result<(), String> {
        // Its marker object, if it has one (deleting a missing key is fine).
        self.delete_key(&prefix_of(dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_matches_the_aws_example() {
        // From the AWS Signature Version 4 documentation ("GET Object").
        let auth = authorization(
            "GET",
            "/test.txt",
            "",
            &[
                ("host".into(), "examplebucket.s3.amazonaws.com".into()),
                ("range".into(), "bytes=0-9".into()),
                ("x-amz-content-sha256".into(), EMPTY_SHA256.into()),
                ("x-amz-date".into(), "20130524T000000Z".into()),
            ],
            EMPTY_SHA256,
            "20130524T000000Z",
            "us-east-1",
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        );
        assert!(
            auth.ends_with(
                "Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
            ),
            "{auth}"
        );
        assert!(auth.contains("SignedHeaders=host;range;x-amz-content-sha256;x-amz-date"));
    }

    #[test]
    fn lists_files_and_folders() {
        let xml = "<ListBucketResult><Prefix>docs/</Prefix>\
            <Contents><Key>docs/</Key><Size>0</Size></Contents>\
            <Contents><Key>docs/a &amp; b.txt</Key><Size>5</Size><LastModified>2024-01-02T03:04:05.000Z</LastModified></Contents>\
            <CommonPrefixes><Prefix>docs/sub/</Prefix></CommonPrefixes></ListBucketResult>";
        let entries = parse_list(xml, "docs/");
        assert_eq!(
            entries,
            vec![
                RemoteEntry {
                    name: "a & b.txt".into(),
                    is_dir: false,
                    size: Some(5),
                    modified: Some(1704164645)
                },
                RemoteEntry {
                    name: "sub".into(),
                    is_dir: true,
                    size: None,
                    modified: None
                },
            ]
        );
        assert_eq!(prefix_of("/"), "");
        assert_eq!(prefix_of("/docs"), "docs/");
    }
}
