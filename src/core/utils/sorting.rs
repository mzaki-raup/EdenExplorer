use crate::core::fs::FileItem;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering::{Equal, Greater, Less};

#[derive(PartialEq, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum SortColumn {
    Name,
    Size,
    Modified,
    Created,
    Type,
    Deleted,
    OriginalDirectory,
}

#[derive(PartialEq, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SortKey {
    pub column: SortColumn,
    pub ascending: bool,
}

/// Case-insensitive order, the same as comparing `to_lowercase()` copies
/// but without allocating (sorting compares each name many times).
fn cmp_ignore_case(a: &str, b: &str) -> std::cmp::Ordering {
    if a.is_ascii() && b.is_ascii() {
        return a
            .bytes()
            .map(|c| c.to_ascii_lowercase())
            .cmp(b.bytes().map(|c| c.to_ascii_lowercase()));
    }
    a.chars().flat_map(char::to_lowercase).cmp(b.chars().flat_map(char::to_lowercase))
}

fn extension_of(f: &FileItem) -> &str {
    f.path.extension().and_then(|s| s.to_str()).unwrap_or("")
}

/// The list order for `keys`: folders first, then each key in turn, then
/// name order for ties.
pub fn compare_files(a: &FileItem, b: &FileItem, keys: &[SortKey]) -> std::cmp::Ordering {
    if a.is_dir != b.is_dir {
        return if a.is_dir { Less } else { Greater };
    }

    for key in keys {
        let ord = match key.column {
            SortColumn::Name => cmp_ignore_case(&a.name, &b.name),
            SortColumn::Size => a.file_size.unwrap_or(0).cmp(&b.file_size.unwrap_or(0)),
            SortColumn::Modified => a.modified_time_raw.cmp(&b.modified_time_raw),
            SortColumn::Created => a.created_time_raw.cmp(&b.created_time_raw),
            SortColumn::Deleted => a.deleted_time_raw.cmp(&b.deleted_time_raw),
            SortColumn::OriginalDirectory => match (a.original_directory.as_deref(), b.original_directory.as_deref()) {
                (Some(x), Some(y)) => cmp_ignore_case(x, y),
                (x, y) => x.is_some().cmp(&y.is_some()),
            },
            SortColumn::Type => match (a.is_dir, b.is_dir) {
                (true, false) => Less,
                (false, true) => Greater,
                (true, true) => cmp_ignore_case(&a.name, &b.name),
                (false, false) => cmp_ignore_case(extension_of(a), extension_of(b)),
            },
        };

        if ord != Equal {
            return if key.ascending { ord } else { ord.reverse() };
        }
    }

    // Ties (same folder, size, date, ...) in name order.
    cmp_ignore_case(&a.name, &b.name)
}

pub fn sort_files_by_keys(files: &mut [FileItem], keys: &[SortKey]) {
    files.sort_by(|a, b| compare_files(a, b, keys));
}

/// Adds `batch` to the already sorted `files`, keeping them sorted: the
/// batch is sorted on its own and merged in, so items arriving in many
/// small batches (a big folder, the flat view) cost a pass over the list
/// each time instead of a full sort.
pub fn merge_sorted(files: &mut Vec<FileItem>, mut batch: Vec<FileItem>, keys: &[SortKey]) {
    if batch.is_empty() {
        return;
    }
    sort_files_by_keys(&mut batch, keys);
    let fits_after = files
        .last()
        .is_none_or(|last| compare_files(last, &batch[0], keys) != Greater);
    if fits_after {
        files.append(&mut batch);
        return;
    }
    let old = std::mem::take(files);
    files.reserve(old.len() + batch.len());
    let mut old = old.into_iter().peekable();
    let mut new = batch.into_iter().peekable();
    while let (Some(a), Some(b)) = (old.peek(), new.peek()) {
        // Equal items keep the earlier one first.
        if compare_files(a, b, keys) != Greater {
            files.extend(old.next());
        } else {
            files.extend(new.next());
        }
    }
    files.extend(old);
    files.extend(new);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn item(name: &str, is_dir: bool, size: u64) -> FileItem {
        FileItem {
            name: name.to_string(),
            path: PathBuf::from(format!(r"C:\x\{name}")),
            is_dir,
            is_hidden: false,
            recycle_bin_pidl: None,
            file_size: Some(size),
            modified_time: None,
            created_time: None,
            deleted_time: None,
            modified_time_raw: None,
            created_time_raw: None,
            deleted_time_raw: None,
            original_directory: None,
            total_space: None,
            free_space: None,
        }
    }

    #[test]
    fn ignores_case_like_lowercase_copies() {
        for (a, b) in [("abc", "ABD"), ("Zeta", "alpha"), ("Éclair", "éclair"), ("a", "A"), ("ß", "SS"), ("İx", "i")] {
            assert_eq!(cmp_ignore_case(a, b), a.to_lowercase().cmp(&b.to_lowercase()), "{a} {b}");
        }
    }

    #[test]
    fn merging_batches_matches_one_sort() {
        let keys = [SortKey { column: SortColumn::Size, ascending: false }];
        let all: Vec<FileItem> = (0..500)
            .map(|i| item(&format!("f{}", (i * 7919) % 503), i % 9 == 0, ((i * 31) % 17) as u64))
            .collect();
        let mut expected = all.clone();
        sort_files_by_keys(&mut expected, &keys);
        let mut merged = Vec::new();
        for chunk in all.chunks(37) {
            merge_sorted(&mut merged, chunk.to_vec(), &keys);
        }
        let names = |v: &[FileItem]| v.iter().map(|f| (f.name.clone(), f.is_dir)).collect::<Vec<_>>();
        assert_eq!(names(&merged), names(&expected));
    }
}
