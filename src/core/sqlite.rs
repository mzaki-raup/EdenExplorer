//! Reading a SQLite database. **Read-only, by construction.**
//!
//! A `.db` file is browsed as a list of tables and views, and each of those as
//! a grid of rows - the same "a store that isn't a filesystem, shown like
//! one" idea `remote::redis` already uses, except this one is an ordinary file
//! on disk rather than a connection.
//!
//! Three decisions worth knowing before changing anything here:
//!
//! * **The connection is opened `SQLITE_OPEN_READ_ONLY`, and nothing in this
//!   module issues a statement that could write.** A file manager's reflexes
//!   are fast and destructive - select-all then Delete is muscle memory - and
//!   a database has no Recycle Bin and no undo, so the ordinary file gestures
//!   must not be able to reach one. Writing, if it is ever added, needs its
//!   own deliberate confirmation rather than inheriting these call sites.
//! * **No connection is held open.** Every call opens, reads and closes. The
//!   file almost always belongs to a program that is still running, and
//!   keeping a handle on it would block that program for as long as a tab
//!   happened to be pointed at it.
//! * **Table and column names are quoted, never interpolated raw.** They come
//!   out of the file being opened, which is untrusted input - a table called
//!   `x"; DROP TABLE y; --` is a perfectly legal thing for a file on a network
//!   share to contain.

use rusqlite::{Connection, OpenFlags, types::ValueRef};
use std::path::{Path, PathBuf};

/// Longest text kept from one cell. Anything longer is cut with an ellipsis:
/// a grid cell can show a few dozen characters, and a column holding whole
/// documents would otherwise copy megabytes per page for nothing.
pub const MAX_CELL_TEXT: usize = 4096;

/// Rows read in one page.
pub const PAGE_ROWS: usize = 500;

/// Counting stops here. `COUNT(*)` is a full scan, and for a view it runs the
/// view's own query, which can be arbitrarily expensive - so every count is
/// bounded and reported as "this many or more" once it hits the cap.
pub const MAX_COUNT: i64 = 1_000_000;

/// Values longer than this fail the read instead of being allocated. Nothing
/// here displays more than `MAX_CELL_TEXT` anyway, and a corrupt (or merely
/// unusual) row claiming a gigabyte-long value should not be able to make the
/// app ask for a gigabyte.
const VALUE_LIMIT: i32 = 32 * 1024 * 1024;

/// How long to wait behind a writer before giving up on a locked database.
const BUSY_TIMEOUT_MS: u32 = 2_000;

/// Every SQLite file starts with this, including the trailing NUL.
const MAGIC: &[u8] = b"SQLite format 3\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Table,
    View,
}

/// A bounded row count - `capped` means the real total is this or larger (see
/// `MAX_COUNT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowCount {
    pub value: i64,
    pub capped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectInfo {
    pub name: String,
    pub kind: ObjectKind,
    /// `None` when the count itself failed - a view over a missing table, say.
    /// The object is still listed, because it does exist.
    pub rows: Option<RowCount>,
}

/// One value, reduced to what a grid can show.
#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Null,
    Integer(i64),
    Real(f64),
    /// Already truncated to `MAX_CELL_TEXT`; `truncated` records that it was.
    Text { text: String, truncated: bool },
    /// Only the length. Blob bytes are never copied out of SQLite.
    Blob(usize),
}

impl Cell {
    /// What the grid draws. `NULL` is deliberately rendered as a word rather
    /// than an empty cell, since an empty string is a different value and the
    /// two must not look the same.
    pub fn display(&self) -> String {
        match self {
            Cell::Null => "NULL".to_string(),
            Cell::Integer(n) => n.to_string(),
            Cell::Real(f) => f.to_string(),
            Cell::Text { text, truncated } => {
                if *truncated {
                    format!("{text}…")
                } else {
                    text.clone()
                }
            }
            Cell::Blob(len) => format!("<{len} bytes>"),
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Cell::Null)
    }
}

/// One page of rows, plus the column names they line up with.
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

/// Extensions that make a file browsable as a database.
///
/// Deliberately extension-gated rather than sniffed, because `split` below
/// runs against ordinary paths and must stay cheap - and because a path has
/// to be recognisable *as* a database path for navigation, Back, and the
/// breadcrumb to keep working long after the file itself was last looked at.
/// A real database with no extension (Chrome's `History`) therefore isn't
/// browsable; it still previews, and copies and renames like any other file.
const BROWSABLE_EXTENSIONS: [&str; 6] = ["db", "db3", "sqlite", "sqlite3", "s3db", "sl3"];

fn has_database_extension(name: &str) -> bool {
    let Some((_, ext)) = name.rsplit_once('.') else {
        return false;
    };
    BROWSABLE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
}

/// Whether opening this file should show its tables rather than hand it to
/// whatever program owns the extension. Checks the header too, so a `.db`
/// that turns out to be some other format is left alone.
pub fn is_browsable_database(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(has_database_extension)
        && path.is_file()
        && looks_like_sqlite(path)
}

/// Splits a path that reaches into a database: `app.db` is the database
/// itself (object `None`), `app.db\customers` is the `customers` table in it.
///
/// Same shape as `archive_view::split`, and cheap for the same reason: an
/// ordinary path has no component named like a database, so nothing is asked
/// of the file system at all. Anything deeper than one object below the
/// database names no real place - tables don't nest.
pub fn split(path: &Path) -> Option<(PathBuf, Option<String>)> {
    let names_database = path
        .components()
        .any(|c| c.as_os_str().to_str().is_some_and(has_database_extension));
    if !names_database {
        return None;
    }
    for ancestor in path.ancestors() {
        let Some(name) = ancestor.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !has_database_extension(name) || !ancestor.is_file() {
            continue;
        }
        let inner: Vec<String> = path
            .strip_prefix(ancestor)
            .ok()?
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        return match inner.len() {
            0 => Some((ancestor.to_path_buf(), None)),
            1 => Some((ancestor.to_path_buf(), Some(inner[0].clone()))),
            _ => None,
        };
    }
    None
}

/// Cheap check that a file is a SQLite database, by its own magic header
/// rather than its extension - `.db`/`.sqlite`/`.sqlite3` are conventions, and
/// plenty of real databases use neither (Firefox's `places.sqlite` does,
/// Chrome's `History` does not).
pub fn looks_like_sqlite(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 16];
    match file.read_exact(&mut head) {
        Ok(()) => head == MAGIC,
        Err(_) => false,
    }
}

/// Wraps an identifier for use in a statement. SQLite quotes with `"`, and an
/// embedded `"` is escaped by doubling it.
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Whether `suffix` exists beside the database (`app.db-wal`, `app.db-journal`).
fn sidecar_exists(path: &Path, suffix: &str) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    path.with_file_name(format!("{name}{suffix}")).exists()
}

/// The `file:` URI for `path`, or `None` when it isn't a plain local path
/// this can encode safely.
///
/// Only used to pass `immutable=1` (see `open`). A UNC path would have to be
/// encoded into the URI's authority, which is fiddly and rare enough not to
/// be worth getting subtly wrong - those fall back to the ordinary open.
fn immutable_uri(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    if text.starts_with(r"\") || text.starts_with("//") {
        return None;
    }
    let mut encoded = String::with_capacity(text.len() + 16);
    for ch in text.chars() {
        match ch {
            '\\' => encoded.push('/'),
            '%' => encoded.push_str("%25"),
            '?' => encoded.push_str("%3f"),
            '#' => encoded.push_str("%23"),
            other => encoded.push(other),
        }
    }
    Some(format!("file:{encoded}?immutable=1"))
}

/// Opens `path` read-only. Never creates the database, and never upgrades to
/// a writable handle - see the module docs.
///
/// **A read-only connection is not enough to leave the folder alone.** A
/// database in WAL mode needs a shared-memory index to be read, and SQLite
/// creates `app.db-shm` (and an empty `app.db-wal`) beside the file if they
/// aren't there - so merely *looking* at a database wrote two files into the
/// user's folder, which is not something a file manager may do. `immutable=1`
/// tells SQLite the file cannot change under it, so it reads the main file
/// directly and touches nothing.
///
/// That is only true when nothing is pending: if a `-wal` or `-journal` file
/// exists, rows live outside the main file and `immutable=1` would silently
/// show stale content - the exact failure that ruled out reading the file
/// format by hand. In that case the ordinary read-only open is used, and the
/// sidecars it needs already exist, because whatever wrote them made them.
pub(crate) fn open(path: &Path) -> Result<Connection, String> {
    let pending = sidecar_exists(path, "-wal") || sidecar_exists(path, "-journal");
    let uri = (!pending).then(|| immutable_uri(path)).flatten();

    let conn = match uri {
        Some(uri) => {
            let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_URI;
            Connection::open_with_flags(uri, flags)
        }
        None => Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ),
    }
    .map_err(|e| explain(path, &e))?;
    conn.busy_handler(Some(|_| false))
        .and_then(|()| conn.busy_timeout(std::time::Duration::from_millis(BUSY_TIMEOUT_MS as u64)))
        .map_err(|e| e.to_string())?;
    // Belt and braces: the open flag already forbids writing, but this also
    // rejects anything that would merely *try*, which makes a mistake here
    // fail loudly in a test rather than quietly at a user's database.
    conn.pragma_update(None, "query_only", true)
        .map_err(|e| e.to_string())?;
    conn.set_db_config(
        rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE,
        true,
    )
    .map_err(|e| e.to_string())?;
    conn.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH, VALUE_LIMIT)
        .map_err(|e| e.to_string())?;
    Ok(conn)
}

/// Turns a `rusqlite` failure into something a person can act on.
///
/// The one worth the trouble is a WAL-mode database: opening one read-only
/// needs a writable `-shm` beside it, so a database whose folder is read-only
/// (or that is on a share, or that a running program holds) fails with a bare
/// "unable to open database file" that says nothing about why.
fn explain(path: &Path, error: &rusqlite::Error) -> String {
    let code = match error {
        rusqlite::Error::SqliteFailure(e, _) => Some(e.code),
        _ => None,
    };
    let wal_beside = path
        .parent()
        .map(|dir| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            dir.join(format!("{name}-wal")).exists()
        })
        .unwrap_or(false);

    match code {
        Some(rusqlite::ErrorCode::NotADatabase) => {
            "This file isn't a SQLite database, or it's encrypted.".to_string()
        }
        Some(rusqlite::ErrorCode::CannotOpen) if wal_beside => {
            "This database is in WAL mode and couldn't be opened read-only - another program \
             may have it open, or its folder isn't writable. Copy it somewhere else to view it."
                .to_string()
        }
        Some(rusqlite::ErrorCode::DatabaseBusy) | Some(rusqlite::ErrorCode::DatabaseLocked) => {
            "The database is locked by another program.".to_string()
        }
        Some(rusqlite::ErrorCode::DatabaseCorrupt) => {
            "This database is damaged and can't be read.".to_string()
        }
        _ => error.to_string(),
    }
}

/// The tables and views in `path`, with a bounded row count for each.
///
/// SQLite's own bookkeeping objects are left out: everything under the
/// reserved `sqlite_` prefix (`sqlite_sequence`, `sqlite_stat1`) plus the
/// shadow tables full-text and r-tree indexes keep beside themselves, which
/// are implementation detail rather than anything a person put there.
pub fn objects(path: &Path) -> Result<Vec<ObjectInfo>, String> {
    let conn = open(path)?;
    let mut listed: Vec<(String, ObjectKind)> = {
        let mut stmt = conn
            .prepare("SELECT name, type FROM sqlite_master WHERE type IN ('table','view')")
            .map_err(|e| explain(path, &e))?;
        let rows = stmt
            .query_map([], |row| {
                let name: String = row.get(0)?;
                let kind: String = row.get(1)?;
                Ok((name, kind))
            })
            .map_err(|e| explain(path, &e))?;

        let mut out = Vec::new();
        for row in rows {
            let (name, kind) = row.map_err(|e| explain(path, &e))?;
            if name.starts_with("sqlite_") {
                continue;
            }
            out.push((
                name,
                if kind == "view" {
                    ObjectKind::View
                } else {
                    ObjectKind::Table
                },
            ));
        }
        out
    };

    let shadows = shadow_table_names(&conn);
    listed.retain(|(name, _)| !shadows.contains(name));
    listed.sort_by_key(|(name, _)| name.to_lowercase());

    Ok(listed
        .into_iter()
        .map(|(name, kind)| {
            let rows = count_rows(&conn, &name).ok();
            ObjectInfo { name, kind, rows }
        })
        .collect())
}

/// The tables a virtual table keeps beside itself (`x_content`, `x_data`,
/// `x_idx`, ...). They're listed in `sqlite_master` as ordinary tables, so the
/// only way to recognise them is to know which virtual tables exist and strip
/// anything sharing their name plus an underscore.
fn shadow_table_names(conn: &Connection) -> std::collections::HashSet<String> {
    let mut shadows = std::collections::HashSet::new();
    let Ok(mut stmt) = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND sql LIKE 'CREATE VIRTUAL TABLE%'",
    ) else {
        return shadows;
    };
    let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) else {
        return shadows;
    };
    let virtuals: Vec<String> = rows.flatten().collect();
    let Ok(mut all) = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table'") else {
        return shadows;
    };
    let Ok(names) = all.query_map([], |row| row.get::<_, String>(0)) else {
        return shadows;
    };
    for name in names.flatten() {
        if virtuals
            .iter()
            .any(|v| name.len() > v.len() + 1 && name.starts_with(v) && name.as_bytes()[v.len()] == b'_')
        {
            shadows.insert(name);
        }
    }
    shadows
}

/// A bounded `COUNT(*)` - see `MAX_COUNT`.
fn count_rows(conn: &Connection, object: &str) -> Result<RowCount, rusqlite::Error> {
    let sql = format!(
        "SELECT COUNT(*) FROM (SELECT 1 FROM {} LIMIT {})",
        quote_ident(object),
        MAX_COUNT + 1
    );
    let value: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(RowCount {
        value: value.min(MAX_COUNT),
        capped: value > MAX_COUNT,
    })
}

/// Row count for one object, opening the file itself.
pub fn count(path: &Path, object: &str) -> Result<RowCount, String> {
    let conn = open(path)?;
    count_rows(&conn, object).map_err(|e| explain(path, &e))
}

/// Reads `limit` rows of `object` starting at `offset`.
///
/// `OFFSET` gets slower the deeper into a large table it goes, since SQLite
/// still walks the rows it skips. That's accepted here: it is correct for
/// every object (a view has no rowid to seek by), and a person paging far
/// enough into a million-row table for it to matter is better served by the
/// filter this view will grow than by a faster skip.
pub fn page(path: &Path, object: &str, offset: usize, limit: usize) -> Result<Page, String> {
    let conn = open(path)?;
    let sql = format!(
        "SELECT * FROM {} LIMIT ?1 OFFSET ?2",
        quote_ident(object)
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| explain(path, &e))?;
    let columns: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
    let width = columns.len();

    let mut rows = Vec::new();
    let mut cursor = stmt
        .query(rusqlite::params![limit as i64, offset as i64])
        .map_err(|e| explain(path, &e))?;
    while let Some(row) = cursor.next().map_err(|e| explain(path, &e))? {
        let mut cells = Vec::with_capacity(width);
        for index in 0..width {
            cells.push(read_cell(row, index).map_err(|e| explain(path, &e))?);
        }
        rows.push(cells);
    }

    Ok(Page { columns, rows })
}

fn read_cell(row: &rusqlite::Row<'_>, index: usize) -> Result<Cell, rusqlite::Error> {
    Ok(match row.get_ref(index)? {
        ValueRef::Null => Cell::Null,
        ValueRef::Integer(n) => Cell::Integer(n),
        ValueRef::Real(f) => Cell::Real(f),
        ValueRef::Blob(bytes) => Cell::Blob(bytes.len()),
        ValueRef::Text(bytes) => {
            let text = String::from_utf8_lossy(bytes);
            // Truncate on a character boundary, not a byte one.
            if text.chars().count() > MAX_CELL_TEXT {
                let cut: String = text.chars().take(MAX_CELL_TEXT).collect();
                Cell::Text {
                    text: cut,
                    truncated: true,
                }
            } else {
                Cell::Text {
                    text: text.into_owned(),
                    truncated: false,
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch database built with a *separate*, writable connection - the
    /// module's own `open` can't create one, which is the point.
    struct Scratch {
        dir: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "eden-sqlite-test-{}-{name}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch { dir }
        }

        fn db(&self, sql: &str) -> PathBuf {
            let path = self.dir.join("test.db");
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(sql).unwrap();
            drop(conn);
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }


    #[test]
    fn a_database_path_splits_into_the_file_and_the_object_inside_it() {
        let scratch = Scratch::new("split");
        let path = scratch.db("CREATE TABLE t (v INTEGER);");

        assert_eq!(split(&path), Some((path.clone(), None)));
        assert_eq!(
            split(&path.join("people")),
            Some((path.clone(), Some("people".to_string())))
        );
        // Tables don't nest, so nothing below one names a real place.
        assert_eq!(split(&path.join("people").join("deeper")), None);
    }

    #[test]
    fn an_ordinary_path_is_not_treated_as_a_database_path() {
        // No component named like a database, so this must not touch the disk
        // at all, let alone claim the path.
        assert_eq!(split(Path::new(r"C:\Users\me\Documents\notes.txt")), None);
        assert_eq!(split(Path::new(r"C:\Users\me")), None);
    }

    #[test]
    fn a_folder_that_merely_ends_in_db_is_still_a_folder() {
        let scratch = Scratch::new("folder");
        let dir = scratch.dir.join("backups.db");
        std::fs::create_dir_all(dir.join("inner")).unwrap();
        // `backups.db` is a directory here, so the path below it is ordinary -
        // `split` checks `is_file`, not just the name.
        assert_eq!(split(&dir.join("inner")), None);
        assert!(!is_browsable_database(&dir));
    }

    #[test]
    fn a_file_with_a_database_extension_but_the_wrong_contents_is_not_browsable() {
        let scratch = Scratch::new("fake");
        let fake = scratch.dir.join("pretend.sqlite");
        std::fs::write(&fake, b"not actually a database at all").unwrap();
        assert!(!is_browsable_database(&fake));

        let real = scratch.db("CREATE TABLE t (v INTEGER);");
        assert!(is_browsable_database(&real));
    }

    #[test]
    fn tables_and_views_are_listed_with_their_row_counts() {
        let scratch = Scratch::new("listing");
        let path = scratch.db(
            "CREATE TABLE people (id INTEGER, name TEXT);
             INSERT INTO people VALUES (1, 'ada'), (2, 'alan'), (3, 'grace');
             CREATE TABLE empty_one (x INTEGER);
             CREATE VIEW just_ada AS SELECT * FROM people WHERE name = 'ada';",
        );

        let objects = objects(&path).unwrap();
        let names: Vec<&str> = objects.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, vec!["empty_one", "just_ada", "people"]);

        let people = &objects[2];
        assert_eq!(people.kind, ObjectKind::Table);
        assert_eq!(
            people.rows,
            Some(RowCount {
                value: 3,
                capped: false
            })
        );
        assert_eq!(objects[1].kind, ObjectKind::View);
        assert_eq!(
            objects[1].rows,
            Some(RowCount {
                value: 1,
                capped: false
            })
        );
        assert_eq!(objects[0].rows.unwrap().value, 0);
    }

    #[test]
    fn sqlite_internal_and_virtual_table_shadows_are_hidden() {
        let scratch = Scratch::new("internal");
        let path = scratch.db(
            "CREATE TABLE a (id INTEGER PRIMARY KEY AUTOINCREMENT, t TEXT);
             INSERT INTO a (t) VALUES ('x');
             CREATE VIRTUAL TABLE docs USING fts5(body);
             INSERT INTO docs VALUES ('hello');",
        );

        let names: Vec<String> = objects(&path)
            .unwrap()
            .into_iter()
            .map(|o| o.name)
            .collect();
        // `sqlite_sequence` exists because of AUTOINCREMENT, and fts5 creates
        // docs_data/docs_idx/docs_content/... - none of which a person put
        // there. The virtual table itself stays.
        assert!(names.contains(&"a".to_string()));
        assert!(names.contains(&"docs".to_string()));
        assert!(
            !names.iter().any(|n| n.starts_with("sqlite_")),
            "internal tables leaked into the listing: {names:?}"
        );
        assert!(
            !names.iter().any(|n| n.starts_with("docs_")),
            "fts5 shadow tables leaked into the listing: {names:?}"
        );
    }

    #[test]
    fn every_value_kind_survives_the_round_trip() {
        let scratch = Scratch::new("values");
        let path = scratch.db(
            "CREATE TABLE things (i INTEGER, r REAL, t TEXT, b BLOB, n INTEGER);
             INSERT INTO things VALUES (42, 1.5, 'hello', x'00010203', NULL);",
        );

        let page = page(&path, "things", 0, 10).unwrap();
        assert_eq!(page.columns, vec!["i", "r", "t", "b", "n"]);
        assert_eq!(
            page.rows[0],
            vec![
                Cell::Integer(42),
                Cell::Real(1.5),
                Cell::Text {
                    text: "hello".into(),
                    truncated: false
                },
                Cell::Blob(4),
                Cell::Null,
            ]
        );
        // An empty string and NULL must not read the same on screen.
        assert_eq!(page.rows[0][4].display(), "NULL");
        assert_eq!(page.rows[0][3].display(), "<4 bytes>");
    }

    #[test]
    fn a_long_text_cell_is_truncated_rather_than_copied_whole() {
        let scratch = Scratch::new("long");
        let path = scratch.db(
            "CREATE TABLE t (v TEXT);
             INSERT INTO t VALUES (printf('%.*c', 100000, 'x'));",
        );

        let page = page(&path, "t", 0, 1).unwrap();
        let Cell::Text { text, truncated } = &page.rows[0][0] else {
            panic!("expected text, got {:?}", page.rows[0][0]);
        };
        assert!(truncated);
        assert_eq!(text.chars().count(), MAX_CELL_TEXT);
    }

    #[test]
    fn paging_walks_the_table_without_repeating_or_skipping_a_row() {
        let scratch = Scratch::new("paging");
        let path = scratch.db(
            "CREATE TABLE n (v INTEGER);
             WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 25)
             INSERT INTO n SELECT x FROM c;",
        );

        let mut seen = Vec::new();
        for offset in (0..25).step_by(10) {
            let page = page(&path, "n", offset, 10).unwrap();
            for row in page.rows {
                let Cell::Integer(v) = row[0] else {
                    panic!("expected an integer");
                };
                seen.push(v);
            }
        }
        assert_eq!(seen, (1..=25).collect::<Vec<i64>>());
        // Past the end is empty, not an error.
        assert!(page(&path, "n", 100, 10).unwrap().rows.is_empty());
    }

    #[test]
    fn a_hostile_table_name_is_quoted_rather_than_interpolated() {
        let scratch = Scratch::new("quoting");
        // The name below is exactly what an injection would need to break out
        // of: a quote, a statement separator, and a comment.
        let path = scratch.db(
            "CREATE TABLE \"ev\"\"il; DROP TABLE keep; --\" (v INTEGER);
             INSERT INTO \"ev\"\"il; DROP TABLE keep; --\" VALUES (7);
             CREATE TABLE keep (v INTEGER);
             INSERT INTO keep VALUES (1);",
        );

        let evil = page(&path, "ev\"il; DROP TABLE keep; --", 0, 10).unwrap();
        assert_eq!(evil.rows[0][0], Cell::Integer(7));
        // The table the name tried to drop is still there, with its row.
        let keep = page(&path, "keep", 0, 10).unwrap();
        assert_eq!(keep.rows[0][0], Cell::Integer(1));
    }

    #[test]
    fn a_column_name_with_quotes_and_spaces_still_reads() {
        let scratch = Scratch::new("columns");
        let path = scratch.db(
            "CREATE TABLE t (\"odd \"\"name\" TEXT, \"with space\" INTEGER);
             INSERT INTO t VALUES ('a', 1);",
        );
        let page = page(&path, "t", 0, 10).unwrap();
        assert_eq!(page.columns, vec!["odd \"name", "with space"]);
    }

    #[test]
    fn the_connection_cannot_write_even_when_asked_directly() {
        let scratch = Scratch::new("readonly");
        let path = scratch.db("CREATE TABLE t (v INTEGER); INSERT INTO t VALUES (1);");

        let conn = open(&path).unwrap();
        // The whole module depends on this being impossible, so pin it rather
        // than trusting the open flag to stay as it is.
        assert!(conn.execute("INSERT INTO t VALUES (2)", []).is_err());
        assert!(conn.execute("DELETE FROM t", []).is_err());
        assert!(conn.execute_batch("DROP TABLE t").is_err());
        drop(conn);

        // And nothing was written.
        assert_eq!(count(&path, "t").unwrap().value, 1);
    }

    #[test]
    fn a_file_that_is_not_a_database_is_refused_by_name_and_by_header() {
        let scratch = Scratch::new("notadb");
        let path = scratch.dir.join("not-a-db.sqlite");
        std::fs::write(&path, b"this is just a text file, honest").unwrap();

        assert!(!looks_like_sqlite(&path));
        let error = objects(&path).unwrap_err();
        assert!(
            error.contains("isn't a SQLite database"),
            "unhelpful error: {error}"
        );
    }

    #[test]
    fn a_real_database_is_recognised_by_its_header() {
        let scratch = Scratch::new("magic");
        let path = scratch.db("CREATE TABLE t (v INTEGER);");
        assert!(looks_like_sqlite(&path));
        // An extension proves nothing either way - this one has none at all.
        let renamed = scratch.dir.join("no-extension");
        std::fs::copy(&path, &renamed).unwrap();
        assert!(looks_like_sqlite(&renamed));
    }



    #[test]
    fn a_local_path_becomes_a_uri_with_its_separators_and_specials_encoded() {
        assert_eq!(
            immutable_uri(Path::new(r"C:\data\app.db")).as_deref(),
            Some("file:C:/data/app.db?immutable=1")
        );
        assert_eq!(
            immutable_uri(Path::new(r"C:\odd #1\100%\what?.db")).as_deref(),
            Some("file:C:/odd %231/100%25/what%3f.db?immutable=1")
        );
        // A share is left to the ordinary open rather than encoded by guess.
        assert_eq!(immutable_uri(Path::new(r"\\server\share\app.db")), None);
    }

    #[test]
    fn reading_a_closed_wal_database_leaves_no_new_files_beside_it() {
        let scratch = Scratch::new("sidecars");
        let path = scratch.dir.join("wal.db");
        {
            let writer = Connection::open(&path).unwrap();
            writer.pragma_update(None, "journal_mode", "WAL").unwrap();
            writer
                .execute_batch("CREATE TABLE t (v INTEGER); INSERT INTO t VALUES (1), (2), (3);")
                .unwrap();
        }
        // A clean close checkpoints and removes the -wal/-shm pair; start from
        // there, so anything present afterwards was created by *reading*.
        let _ = std::fs::remove_file(scratch.dir.join("wal.db-wal"));
        let _ = std::fs::remove_file(scratch.dir.join("wal.db-shm"));

        let before = listing(&scratch.dir);
        assert_eq!(page(&path, "t", 0, 10).unwrap().rows.len(), 3);
        let after = listing(&scratch.dir);

        // "Read-only" has to mean the folder is left as it was found. If this
        // ever fails, a plain look at someone's database is writing files next
        // to it - which is exactly what a file manager must not do.
        assert_eq!(before, after, "reading the database changed the folder");
    }

    fn listing(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().into_owned()))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_wal_database_is_read_including_rows_that_are_still_in_the_wal() {
        let scratch = Scratch::new("wal");
        let path = scratch.dir.join("wal.db");
        let writer = Connection::open(&path).unwrap();
        writer
            .pragma_update(None, "journal_mode", "WAL")
            .unwrap();
        writer
            .execute_batch("CREATE TABLE t (v INTEGER); INSERT INTO t VALUES (1), (2);")
            .unwrap();
        // Deliberately *not* checkpointed and *not* closed: the rows live in
        // the -wal file, which is exactly the case a hand-rolled file-format
        // reader would silently miss. Most real databases a person meets
        // (browsers, Electron apps) are in this mode.
        let page = page(&path, "t", 0, 10).unwrap();
        assert_eq!(page.rows.len(), 2);
        drop(writer);
    }
}
