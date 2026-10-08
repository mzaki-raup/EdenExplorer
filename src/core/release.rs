//! Which published release, if any, is newer than the copy that is running.
//!
//! The release workflow (`.forgejo/workflows/release.yml`) tags a release
//! `v1.0.0` and attaches the program and a `SHA256SUMS.txt` to it; this module
//! works out from the forge's own API whether one of them is worth offering.
//! Pure - no network, no Windows - so all of it is unit-testable;
//! `core::update` does the fetching, verifying and installing.

use serde::Deserialize;

/// Where this copy looks for its own releases. Anonymous read, so no token is
/// needed - the repository is public.
pub const RELEASES_API: &str =
    "https://git.zacx.net/api/v1/repos/owner/eden-explorer/releases?limit=10";

/// The program as published on a release, and the file of checksums beside it.
/// These are the exact asset names the release workflow publishes - a release
/// missing either is never installed automatically.
pub const EXE_ASSET: &str = "EdenExplorer.exe";
pub const SUMS_ASSET: &str = "SHA256SUMS.txt";

/// A release version, as a tag says it (`v1.0.0`, or `1.0.0`). Ordered, so the
/// newest release is simply the largest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
        // Anything after the three numbers (`-rc1`, `+build`) is not a release
        // this understands, which is why the workflow only ever tags vX.Y.Z.
        let mut parts = s.split('.');
        let mut next = || parts.next().filter(|p| !p.is_empty())?.parse::<u32>().ok();
        let (major, minor, patch) = (next()?, next()?, next()?);
        parts.next().is_none().then_some(Version { major, minor, patch })
    }

    /// The version this copy was built as (`Cargo.toml`, via the release
    /// workflow's own version bump).
    pub fn running() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_default()
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A file attached to a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// A release worth offering: newer than what is running, and with the program
/// attached to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub version: Version,
    pub tag: String,
    /// The release's page, for "what's new".
    pub page: String,
    pub notes: String,
    pub exe: Asset,
    /// The checksums beside it. A release without them is never installed
    /// automatically (see `core::update::download`).
    pub sums: Option<Asset>,
}

/// What the forge's releases API answers with. Unknown fields are ignored, so
/// this reads both Forgejo/Gitea and GitHub.
#[derive(Deserialize)]
struct ApiRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    #[serde(default)]
    name: String,
    #[serde(default)]
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

impl ApiRelease {
    fn asset(&self, name: &str) -> Option<Asset> {
        self.assets
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name) && !a.browser_download_url.is_empty())
            .map(|a| Asset {
                name: a.name.clone(),
                url: a.browser_download_url.clone(),
                size: a.size,
            })
    }
}

/// The newest release in the API's answer that is newer than `current`. Drafts,
/// pre-releases, tags that aren't a plain version, and releases with no program
/// attached are all passed over.
pub fn newer_than(json: &str, current: Version) -> Option<Update> {
    let releases: Vec<ApiRelease> = serde_json::from_str(json).ok()?;
    releases
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| {
            let version = Version::parse(&r.tag_name)?;
            if version <= current {
                return None;
            }
            Some(Update {
                version,
                tag: r.tag_name.clone(),
                page: r.html_url.clone(),
                notes: r.body.trim().to_string(),
                exe: r.asset(EXE_ASSET)?,
                sums: r.asset(SUMS_ASSET),
            })
        })
        .max_by_key(|u| u.version)
}

/// The checksum of `name` in a `SHA256SUMS.txt` (lines of "<hex>  <file name>",
/// as `sha256sum` and the release workflow both write them), lowercased.
pub fn sha256_of(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        // `sha256sum` marks a binary-mode file with a leading asterisk.
        let file = file.trim().trim_start_matches('*');
        let hash = hash.trim();
        let ok = file.eq_ignore_ascii_case(name)
            && hash.len() == 64
            && hash.bytes().all(|b| b.is_ascii_hexdigit());
        ok.then(|| hash.to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(major: u32, minor: u32, patch: u32) -> Version {
        Version {
            major,
            minor,
            patch,
        }
    }

    fn release_json(tag: &str) -> String {
        format!(
            concat!(
                r#"[{{"tag_name":"{tag}","draft":false,"prerelease":false,"#,
                r#""html_url":"https://forge/r/{tag}","body":"notes here","assets":["#,
                r#"{{"name":"EdenExplorer.exe","browser_download_url":"https://forge/{tag}/EdenExplorer.exe","size":42}},"#,
                r#"{{"name":"SHA256SUMS.txt","browser_download_url":"https://forge/{tag}/SHA256SUMS.txt","size":90}}"#,
                r#"]}}]"#
            ),
            tag = tag
        )
    }

    #[test]
    fn versions_parse_with_or_without_the_tag_prefix() {
        assert_eq!(Version::parse("v1.0.0"), Some(v(1, 0, 0)));
        assert_eq!(Version::parse("2.10.0"), Some(v(2, 10, 0)));
        assert_eq!(Version::parse(" V0.0.0 "), Some(v(0, 0, 0)));
    }

    #[test]
    fn anything_that_is_not_three_plain_numbers_is_not_a_version() {
        for s in [
            "", "v2", "2.0", "2.0.1.1", "2.0.x", "v1.0.0-rc1", "nightly", "2..1",
        ] {
            assert_eq!(Version::parse(s), None, "{s} should not parse");
        }
    }

    #[test]
    fn versions_order_by_number_not_by_text() {
        assert!(v(2, 10, 0) > v(2, 9, 9));
        assert!(v(3, 0, 0) > v(2, 99, 99));
        assert!(v(1, 0, 1) > v(1, 0, 0));
    }

    #[test]
    fn a_newer_release_with_both_assets_is_offered() {
        let found = newer_than(&release_json("v1.2.0"), v(1, 0, 0)).expect("an update");
        assert_eq!(found.version, v(1, 2, 0));
        assert_eq!(found.tag, "v1.2.0");
        assert_eq!(found.exe.url, "https://forge/v1.2.0/EdenExplorer.exe");
        assert_eq!(found.sums.map(|a| a.name), Some("SHA256SUMS.txt".to_string()));
        assert_eq!(found.notes, "notes here");
    }

    #[test]
    fn the_same_or_an_older_version_is_not_offered() {
        assert_eq!(newer_than(&release_json("v1.0.0"), v(1, 0, 0)), None);
        assert_eq!(newer_than(&release_json("v0.9.0"), v(1, 0, 0)), None);
    }

    #[test]
    fn the_newest_of_several_releases_wins_regardless_of_api_order() {
        let json = concat!(
            r#"[{"tag_name":"v1.1.0","html_url":"h","assets":[{"name":"EdenExplorer.exe","browser_download_url":"u"}]},"#,
            r#"{"tag_name":"v1.10.0","html_url":"h","assets":[{"name":"EdenExplorer.exe","browser_download_url":"u"}]},"#,
            r#"{"tag_name":"v1.2.0","html_url":"h","assets":[{"name":"EdenExplorer.exe","browser_download_url":"u"}]}]"#
        );
        assert_eq!(
            newer_than(json, v(1, 0, 0)).map(|u| u.version),
            Some(v(1, 10, 0))
        );
    }

    #[test]
    fn drafts_prereleases_and_releases_without_the_program_are_passed_over() {
        let draft = r#"[{"tag_name":"v9.0.0","draft":true,"html_url":"h","assets":[{"name":"EdenExplorer.exe","browser_download_url":"u"}]}]"#;
        assert_eq!(newer_than(draft, v(1, 0, 0)), None);

        let pre = r#"[{"tag_name":"v9.0.0","prerelease":true,"html_url":"h","assets":[{"name":"EdenExplorer.exe","browser_download_url":"u"}]}]"#;
        assert_eq!(newer_than(pre, v(1, 0, 0)), None);

        let bare = r#"[{"tag_name":"v9.0.0","html_url":"h","assets":[]}]"#;
        assert_eq!(newer_than(bare, v(1, 0, 0)), None);

        // An asset entry with no download URL is as good as absent.
        let no_url = r#"[{"tag_name":"v9.0.0","html_url":"h","assets":[{"name":"EdenExplorer.exe","browser_download_url":""}]}]"#;
        assert_eq!(newer_than(no_url, v(1, 0, 0)), None);
    }

    #[test]
    fn a_release_without_checksums_is_still_found_but_reports_no_sums() {
        let json = r#"[{"tag_name":"v2.0.0","html_url":"h","assets":[{"name":"EdenExplorer.exe","browser_download_url":"u"}]}]"#;
        let found = newer_than(json, v(1, 0, 0)).expect("an update");
        assert!(found.sums.is_none(), "there is no SHA256SUMS.txt to report");
    }

    #[test]
    fn malformed_json_is_simply_no_update() {
        assert_eq!(newer_than("not json", v(1, 0, 0)), None);
        assert_eq!(newer_than("", v(1, 0, 0)), None);
        assert_eq!(newer_than("{}", v(1, 0, 0)), None);
    }

    #[test]
    fn checksums_are_read_from_the_sha256sum_layout() {
        let sums = concat!(
            "565ae7e15a7fc7a0c3e713a7c39493e37224e5fc727b5fe02f21700708f3bee9  EdenExplorer.exe\n",
            "a1b10af9d614d13be23129afdd6c24cfc5a79170cff758bfda20a100dadde450  EdenExplorer-1.0.0-windows-x64.zip\n"
        );
        assert_eq!(
            sha256_of(sums, "EdenExplorer.exe").as_deref(),
            Some("565ae7e15a7fc7a0c3e713a7c39493e37224e5fc727b5fe02f21700708f3bee9")
        );
        assert_eq!(sha256_of(sums, "nothing.exe"), None);
    }

    #[test]
    fn a_binary_mode_marker_and_upper_case_hash_still_read() {
        let sums = "565AE7E15A7FC7A0C3E713A7C39493E37224E5FC727B5FE02F21700708F3BEE9 *EdenExplorer.exe";
        assert_eq!(
            sha256_of(sums, "EdenExplorer.exe").as_deref(),
            Some("565ae7e15a7fc7a0c3e713a7c39493e37224e5fc727b5fe02f21700708f3bee9")
        );
    }

    #[test]
    fn a_truncated_or_non_hex_checksum_is_refused() {
        assert_eq!(
            sha256_of("deadbeef  EdenExplorer.exe", "EdenExplorer.exe"),
            None
        );
        assert_eq!(
            sha256_of(
                "zzz0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  EdenExplorer.exe",
                "EdenExplorer.exe"
            ),
            None
        );
    }
}
