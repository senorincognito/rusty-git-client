use std::time::Duration;

use serde::Serialize;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LineBlame {
    pub id: String,
    pub short_id: String,
    pub author: String,
    /// Unix seconds.
    pub time: i64,
    pub summary: String,
}

/// Reads `git blame --porcelain -L n,n` output: the header line carries the commit id, then `author`,
/// `author-time` and `summary` lines. `None` for lines that no commit has yet (the all-zero id).
fn parse(out: &str) -> Option<LineBlame> {
    let id = out.lines().next()?.split(' ').next()?;
    if id.len() < 7 || id.bytes().all(|b| b == b'0') {
        return None;
    }
    let field = |name: &str| out.lines().find_map(|l| l.strip_prefix(name)).unwrap_or("").trim().to_string();
    Some(LineBlame {
        id: id.to_string(),
        short_id: id.chars().take(7).collect(),
        author: field("author "),
        time: field("author-time ").parse().unwrap_or(0),
        summary: field("summary "),
    })
}

/// The commit that last changed line `line` (1-based) of the working-directory file, or `None` when the line is
/// not committed yet (edited, or the whole file is untracked). Blames the file as it is on disk, so the line
/// numbers are the ones the viewer shows.
fn blame_line(repo_path: &str, file: &str, line: u32) -> Result<Option<LineBlame>, String> {
    if line == 0 {
        return Err("line numbers start at 1".into());
    }
    let range = format!("{line},{line}");
    let args = ["--literal-pathspecs", "blame", "--porcelain", "-L", &range, "--", file];
    match crate::toolbar::sync::run_git_with(repo_path, &args, Some(Duration::from_secs(30))) {
        Ok(out) => Ok(parse(&out)),
        // A file git doesn't know, or a line past the end of what it blames.
        Err(e) if e.contains("no such path") || e.contains("file ") && e.contains("lines") => Ok(None),
        Err(e) => Err(e),
    }
}

/// Who last changed one line of a file, for the hint beside the selected line in the Files viewer.
#[tauri::command]
pub async fn get_line_blame(path: String, file: String, line: u32) -> Result<Option<LineBlame>, String> {
    tauri::async_runtime::spawn_blocking(move || blame_line(&path, &file, line)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{commit_file, new_repo};

    #[test]
    fn names_the_commit_that_last_changed_a_line() {
        let (dir, repo) = new_repo("blame");
        let first = commit_file(&repo, &dir, "f.txt", "one\ntwo\n", "add f");
        let second = commit_file(&repo, &dir, "f.txt", "one\nTWO\n", "shout two");
        let p = dir.to_str().unwrap();
        let a = blame_line(p, "f.txt", 1).unwrap().unwrap();
        assert_eq!((a.id, a.summary.as_str(), a.author.as_str()), (first.to_string(), "add f", "D"));
        let b = blame_line(p, "f.txt", 2).unwrap().unwrap();
        assert_eq!((b.id, b.short_id.len()), (second.to_string(), 7));
        // An edit that is not committed, and a file git doesn't know.
        std::fs::write(dir.join("f.txt"), "one\nTWO\nthree\n").unwrap();
        assert_eq!(blame_line(p, "f.txt", 3).unwrap(), None);
        std::fs::write(dir.join("new.txt"), "x\n").unwrap();
        assert_eq!(blame_line(p, "new.txt", 1).unwrap(), None);
        assert!(blame_line(p, "f.txt", 0).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
