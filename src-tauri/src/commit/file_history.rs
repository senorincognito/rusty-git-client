use std::time::Duration;

use serde::Serialize;

/// Most commits listed for one file.
const LIMIT: usize = 500;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author: String,
    /// Unix seconds.
    pub time: i64,
    /// What the commit did to the file: new, modified, deleted, renamed, copied or typechange.
    pub status: &'static str,
    /// The file's path in this commit (it differs from the current one before a rename).
    pub path: String,
    /// The path before the commit, for renames and copies.
    pub old_path: Option<String>,
}

/// Parses `git log --format=<RS>id<US>author<US>time<US>subject --name-status` output.
fn parse(out: &str) -> Vec<HistoryEntry> {
    let mut entries = Vec::new();
    for record in out.split('\x1e').filter(|r| !r.trim().is_empty()) {
        let (header, rest) = record.split_once('\n').unwrap_or((record, ""));
        let mut f = header.split('\x1f');
        let (Some(id), Some(author), Some(time), Some(summary)) = (f.next(), f.next(), f.next(), f.next()) else {
            continue;
        };
        // The status line names the file (`M\tpath`, or `R100\told\tnew`). Merges without a change have none.
        let Some(line) = rest.lines().rev().find(|l| !l.trim().is_empty()) else { continue };
        let mut cols = line.split('\t');
        let (Some(code), Some(first)) = (cols.next(), cols.next()) else { continue };
        let second = cols.next();
        let status = match code.chars().next() {
            Some('A') => "new",
            Some('D') => "deleted",
            Some('R') => "renamed",
            Some('C') => "copied",
            Some('T') => "typechange",
            _ => "modified",
        };
        let (path, old_path) = match second {
            Some(new) => (new.to_string(), Some(first.to_string())),
            None => (first.to_string(), None),
        };
        entries.push(HistoryEntry {
            id: id.to_string(),
            short_id: id.chars().take(7).collect(),
            summary: summary.to_string(),
            author: author.to_string(),
            time: time.parse().unwrap_or(0),
            status,
            path,
            old_path,
        });
    }
    entries
}

/// Every commit that changed `file`, newest first, following the file across renames (`git log --follow`).
fn file_history(repo_path: &str, file: &str) -> Result<Vec<HistoryEntry>, String> {
    let limit = format!("-n{LIMIT}");
    let out = crate::toolbar::sync::run_git_with(
        repo_path,
        &[
            "--literal-pathspecs",
            "-c",
            "core.quotepath=false",
            "log",
            "--follow",
            "--no-color",
            &limit,
            "--format=%x1e%H%x1f%an%x1f%at%x1f%s",
            "--name-status",
            "--",
            file,
        ],
        Some(Duration::from_secs(30)),
    )?;
    Ok(parse(&out))
}

/// The commits that touched `file` (a path in the working tree), newest first.
#[tauri::command]
pub async fn get_file_history(path: String, file: String) -> Result<Vec<HistoryEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || file_history(&path, &file)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{commit_file, new_repo};
    use git2::{Repository, Signature};
    use std::path::Path;

    /// Renames `from` to `to` (same content) and commits it on HEAD.
    fn rename_and_commit(repo: &Repository, dir: &Path, from: &str, to: &str, msg: &str) {
        std::fs::rename(dir.join(from), dir.join(to)).unwrap();
        let mut index = repo.index().unwrap();
        index.remove_path(Path::new(from)).unwrap();
        index.add_path(Path::new(to)).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("D", "d@example.com").unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&parent]).unwrap();
    }

    #[test]
    fn lists_the_commits_touching_a_file_across_a_rename() {
        let (dir, repo) = new_repo("filehistory");
        let body = (1..=12).map(|i| format!("line {i}\n")).collect::<String>();
        commit_file(&repo, &dir, "f.txt", &body, "add f");
        commit_file(&repo, &dir, "other.txt", "x", "unrelated");
        commit_file(&repo, &dir, "f.txt", &body.replace("line 3", "line three"), "edit f");
        rename_and_commit(&repo, &dir, "f.txt", "h.txt", "rename f to h");
        commit_file(&repo, &dir, "h.txt", &body.replace("line 3", "line three").replace("line 9", "nine"), "edit h");
        let p = dir.to_str().unwrap();

        let h = file_history(p, "h.txt").unwrap();
        let summaries: Vec<_> = h.iter().map(|e| e.summary.as_str()).collect();
        assert_eq!(summaries, ["edit h", "rename f to h", "edit f", "add f"]);
        let status: Vec<_> = h.iter().map(|e| (e.status, e.path.as_str(), e.old_path.as_deref())).collect();
        assert_eq!(
            status,
            [
                ("modified", "h.txt", None),
                ("renamed", "h.txt", Some("f.txt")),
                ("modified", "f.txt", None),
                ("new", "f.txt", None),
            ]
        );
        assert_eq!(h[0].short_id.len(), 7);
        assert_eq!(h[3].author, "D");

        // A file that never existed has no history.
        assert!(file_history(p, "nope.txt").unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_added_deleted_and_odd_subjects() {
        let out = "\x1eaaaaaaaaaaaa\x1fAnn\x1f100\x1fdelete it\n\nD\tdir/a b.txt\n\x1ebbbbbbbbbbbb\x1fBo\x1f50\x1fadd: it\n\nA\tdir/a b.txt\n";
        let e = parse(out);
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].status, e[0].path.as_str(), e[0].time), ("deleted", "dir/a b.txt", 100));
        assert_eq!((e[1].status, e[1].summary.as_str()), ("new", "add: it"));
    }
}
