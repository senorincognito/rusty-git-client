//! Merge conflicts that are left in place for the conflict view: a conflicted file read as segments, resolved by
//! side or by content, and the state of the merge itself (`get_merge_state`, `abort_merge_cmd`).
//!
//! Only merges (and pulls that merge) stop with their conflicts; the sides are named as in a merge, "ours" is the
//! checked-out branch and "theirs" the branch being merged in. A rebase swaps those two, so reuse needs a mapping.

use std::path::Path;

use git2::{Index, Repository, RepositoryState};
use serde::Serialize;

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

/// A stretch of a conflicted file: text both sides agree on, or one conflict with each side's version.
#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Segment {
    Text { text: String },
    Conflict { ours: String, theirs: String },
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    /// Empty when the file can't be merged line by line (`binary`, or deleted on one side).
    pub segments: Vec<Segment>,
    pub ours_label: String,
    pub theirs_label: String,
    /// False when that side deleted the file.
    pub ours_exists: bool,
    pub theirs_exists: bool,
    pub binary: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MergeState {
    /// The message git prepared for the merge commit, without its comment lines.
    pub message: String,
    /// Files that still have conflicts.
    pub conflicts: usize,
}

/// A marker line is the marker alone or followed by a space and a label (`<<<<<<< HEAD`); it may end in CR or LF.
fn marker(line: &str, ch: char) -> Option<&str> {
    let line = line.trim_end_matches(['\r', '\n']);
    let rest = line.strip_prefix(&ch.to_string().repeat(7))?;
    match rest.strip_prefix(' ') {
        Some(label) => Some(label),
        None if rest.is_empty() => Some(""),
        None => None,
    }
}

/// Splits a file with conflict markers into segments plus the two labels from the marker lines. Every line keeps its
/// own line ending, so joining the segments gives the file back. A diff3 base section is dropped.
fn parse_markers(text: &str) -> Result<(Vec<Segment>, String, String), String> {
    enum Part {
        Text,
        Ours,
        Base,
        Theirs,
    }
    let mut part = Part::Text;
    let (mut common, mut ours, mut theirs) = (String::new(), String::new(), String::new());
    let (mut ours_label, mut theirs_label) = (String::new(), String::new());
    let mut segments = Vec::new();
    for line in text.split_inclusive('\n') {
        match part {
            Part::Text => match marker(line, '<') {
                Some(label) => {
                    if !common.is_empty() {
                        segments.push(Segment::Text { text: std::mem::take(&mut common) });
                    }
                    if ours_label.is_empty() {
                        ours_label = label.to_string();
                    }
                    part = Part::Ours;
                }
                None => common.push_str(line),
            },
            Part::Ours => {
                if marker(line, '|').is_some() {
                    part = Part::Base;
                } else if marker(line, '=').is_some_and(str::is_empty) {
                    part = Part::Theirs;
                } else {
                    ours.push_str(line);
                }
            }
            Part::Base => {
                if marker(line, '=').is_some_and(str::is_empty) {
                    part = Part::Theirs;
                }
            }
            Part::Theirs => match marker(line, '>') {
                Some(label) => {
                    if theirs_label.is_empty() {
                        theirs_label = label.to_string();
                    }
                    segments.push(Segment::Conflict {
                        ours: std::mem::take(&mut ours),
                        theirs: std::mem::take(&mut theirs),
                    });
                    part = Part::Text;
                }
                None => theirs.push_str(line),
            },
        }
    }
    if !matches!(part, Part::Text) {
        return Err("The conflict markers in this file are incomplete".into());
    }
    if !common.is_empty() {
        segments.push(Segment::Text { text: common });
    }
    Ok((segments, ours_label, theirs_label))
}

/// The sides of one conflicted path, if the index has it as conflicted.
fn conflict_of(index: &Index, file: &str) -> Result<git2::IndexConflict, String> {
    for c in index.conflicts().map_err(err)? {
        let c = c.map_err(err)?;
        let entry = c.our.as_ref().or(c.their.as_ref()).or(c.ancestor.as_ref());
        if entry.is_some_and(|e| e.path == file.as_bytes()) {
            return Ok(c);
        }
    }
    Err(format!("{file} has no merge conflict"))
}

/// The conflicted paths, sorted.
fn conflicted_paths(repo: &Repository) -> Result<Vec<String>, String> {
    let mut index = repo.index().map_err(err)?;
    index.read(true).map_err(err)?;
    let mut paths = std::collections::BTreeSet::new();
    for c in index.conflicts().map_err(err)? {
        let c = c.map_err(err)?;
        if let Some(e) = c.our.as_ref().or(c.their.as_ref()).or(c.ancestor.as_ref()) {
            paths.insert(String::from_utf8_lossy(&e.path).into_owned());
        }
    }
    Ok(paths.into_iter().collect())
}

/// After a failed merge: whether git stopped halfway with conflicts left in the working directory.
pub(crate) fn stopped_on_conflicts(path: &str) -> bool {
    Repository::discover(path)
        .is_ok_and(|r| r.state() == RepositoryState::Merge && conflicted_paths(&r).is_ok_and(|p| !p.is_empty()))
}

/// What to tell the user when a merge stopped on conflicts.
pub(crate) fn stopped_message(path: &str) -> String {
    let files = Repository::discover(path).ok().and_then(|r| conflicted_paths(&r).ok()).unwrap_or_default();
    format!(
        "The merge stopped with conflicts in: {}.

Resolve them from the Changes panel (click a file), then commit to finish the merge.",
        files.join(", ")
    )
}

pub(crate) fn merge_state(repo: &Repository) -> Result<Option<MergeState>, String> {
    if repo.state() != RepositoryState::Merge {
        return Ok(None);
    }
    let message = repo.message().unwrap_or_default();
    let message: Vec<&str> = message.lines().filter(|l| !l.starts_with('#')).collect();
    // The merge was started with a full ref name, which git repeats in the message.
    let message = message.join("\n").replace("'refs/heads/", "'").replace("'refs/remotes/", "'");
    Ok(Some(MergeState { message: message.trim().to_string(), conflicts: conflicted_paths(repo)?.len() }))
}

fn head_name(repo: &Repository) -> String {
    repo.head().ok().and_then(|h| h.shorthand().ok().map(str::to_string)).unwrap_or_else(|| "HEAD".into())
}

fn read_conflict(repo: &Repository, file: &str) -> Result<ConflictFile, String> {
    let mut index = repo.index().map_err(err)?;
    index.read(true).map_err(err)?;
    let c = conflict_of(&index, file)?;
    let blob_binary = |e: &Option<git2::IndexEntry>| {
        e.as_ref().and_then(|e| repo.find_blob(e.id).ok()).is_some_and(|b| b.is_binary())
    };
    let workdir = repo.workdir().ok_or("The repository has no working directory")?;
    let text = std::fs::read(workdir.join(file)).ok().and_then(|b| String::from_utf8(b).ok());
    let mut out = ConflictFile {
        segments: Vec::new(),
        ours_label: head_name(repo),
        theirs_label: String::new(),
        ours_exists: c.our.is_some(),
        theirs_exists: c.their.is_some(),
        binary: blob_binary(&c.our)
            || blob_binary(&c.their)
            || (text.is_none() && c.our.is_some() && c.their.is_some()),
    };
    if let (false, true, true, Some(text)) = (out.binary, out.ours_exists, out.theirs_exists, text) {
        let (segments, _, theirs_label) = parse_markers(&text)?;
        out.segments = segments;
        out.theirs_label = theirs_label;
    }
    // The merge was started with a full ref name, which git puts in the marker.
    for prefix in ["refs/heads/", "refs/remotes/"] {
        if let Some(short) = out.theirs_label.strip_prefix(prefix) {
            out.theirs_label = short.to_string();
            break;
        }
    }
    if out.theirs_label.is_empty() {
        out.theirs_label = "incoming".into();
    }
    Ok(out)
}

/// Whether `file` is conflicted in the index and still carries conflict markers: staging it would commit them.
pub(crate) fn has_unresolved_markers(repo: &Repository, file: &str) -> bool {
    let Ok(mut index) = repo.index() else { return false };
    if index.read(true).is_err() || conflict_of(&index, file).is_err() {
        return false;
    }
    let Some(dir) = repo.workdir() else { return false };
    std::fs::read_to_string(dir.join(file)).is_ok_and(|text| {
        parse_markers(&text)
            .is_ok_and(|(segments, _, _)| segments.iter().any(|s| matches!(s, Segment::Conflict { .. })))
    })
}

/// Takes one side of a conflicted file as the result and stages it. System git does the checkout, so line-ending
/// conversion and file modes behave as usual; a side that deleted the file deletes it.
fn resolve_side(repo: &Repository, file: &str, side: &str) -> Result<(), String> {
    let mut index = repo.index().map_err(err)?;
    index.read(true).map_err(err)?;
    let c = conflict_of(&index, file)?;
    let (flag, exists) = match side {
        "ours" => ("--ours", c.our.is_some()),
        "theirs" => ("--theirs", c.their.is_some()),
        other => return Err(format!("Unknown side \"{other}\"")),
    };
    let path = repo.workdir().ok_or("The repository has no working directory")?.to_string_lossy().into_owned();
    let run = |args: &[&str]| crate::toolbar::sync::run_git(&path, args).map(|_| ());
    if exists {
        run(&["--literal-pathspecs", "checkout", flag, "--", file])?;
        run(&["--literal-pathspecs", "add", "--", file])
    } else {
        run(&["--literal-pathspecs", "rm", "--quiet", "--", file])
    }
}

/// Writes `content` as the file and stages it, which marks the conflict as resolved.
fn save_resolution(repo: &Repository, file: &str, content: &str) -> Result<(), String> {
    let mut index = repo.index().map_err(err)?;
    index.read(true).map_err(err)?;
    conflict_of(&index, file)?;
    let workdir = repo.workdir().ok_or("The repository has no working directory")?;
    let target = workdir.join(file);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Could not write {file}: {e}"))?;
    }
    std::fs::write(&target, content).map_err(|e| format!("Could not write {file}: {e}"))?;
    index.add_path(Path::new(file)).map_err(err)?;
    index.write().map_err(err)
}

fn abort_merge(path: &str) -> Result<(), String> {
    let repo = Repository::discover(path).map_err(err)?;
    if repo.state() != RepositoryState::Merge {
        return Err("There is no merge in progress".into());
    }
    crate::toolbar::sync::run_git(path, &["merge", "--abort"]).map(|_| ())
}

async fn blocking<T, F>(path: String, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&Repository) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || f(&Repository::discover(&path).map_err(err)?))
        .await
        .map_err(|e| e.to_string())?
}

/// The merge in progress (message for the commit, files left to resolve), or None.
#[tauri::command]
pub async fn get_merge_state(path: String) -> Result<Option<MergeState>, String> {
    blocking(path, merge_state).await
}

/// A conflicted file as segments for the conflict view.
#[tauri::command]
pub async fn get_conflict(path: String, file: String) -> Result<ConflictFile, String> {
    blocking(path, move |r| read_conflict(r, &file)).await
}

/// Resolves a conflicted file with one side: "ours" (the checked-out branch) or "theirs" (the incoming one).
#[tauri::command]
pub async fn resolve_conflict_side_cmd(path: String, file: String, side: String) -> Result<(), String> {
    blocking(path, move |r| resolve_side(r, &file, &side)).await
}

/// Resolves a conflicted file with the given content.
#[tauri::command]
pub async fn save_resolution_cmd(path: String, file: String, content: String) -> Result<(), String> {
    blocking(path, move |r| save_resolution(r, &file, &content)).await
}

/// Gives up the merge in progress: everything goes back to how it was before it started.
#[tauri::command]
pub async fn abort_merge_cmd(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || abort_merge(&path)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{commit_file, new_repo};
    use git2::build::CheckoutBuilder;
    use std::fs;

    fn switch(repo: &Repository, branch: &str) {
        repo.set_head(&format!("refs/heads/{branch}")).unwrap();
        repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();
    }

    #[test]
    fn markers_become_segments_that_join_back() {
        let text = "a\r\n<<<<<<< HEAD\r\nmine\r\n=======\r\ntheirs 1\r\ntheirs 2\r\n>>>>>>> feature\r\nz\r\n";
        let (segments, ours, theirs) = parse_markers(text).unwrap();
        assert_eq!((ours.as_str(), theirs.as_str()), ("HEAD", "feature"));
        assert_eq!(
            segments,
            [
                Segment::Text { text: "a\r\n".into() },
                Segment::Conflict { ours: "mine\r\n".into(), theirs: "theirs 1\r\ntheirs 2\r\n".into() },
                Segment::Text { text: "z\r\n".into() },
            ]
        );
        // A diff3 base section is skipped; two conflicts in a row and an empty side work.
        let text = "<<<<<<< HEAD\nx\n||||||| base\nb\n=======\n>>>>>>> o\n<<<<<<< HEAD\n=======\ny\n>>>>>>> o\n";
        let (segments, _, _) = parse_markers(text).unwrap();
        assert_eq!(
            segments,
            [
                Segment::Conflict { ours: "x\n".into(), theirs: String::new() },
                Segment::Conflict { ours: String::new(), theirs: "y\n".into() },
            ]
        );
        // Text that merely looks similar is left alone; an unfinished conflict is an error.
        assert_eq!(
            parse_markers("<<<<<<<<< x\n=== \n").unwrap().0,
            [Segment::Text { text: "<<<<<<<<< x\n=== \n".into() }]
        );
        assert!(parse_markers("<<<<<<< HEAD\nx\n=======\n").is_err());
    }

    /// `main` and `other` both rewrite f.txt (and g.txt, which `other` deletes while `main` edits it); main is checked out.
    fn conflicted_merge(name: &str) -> (std::path::PathBuf, Repository) {
        let (dir, repo) = new_repo(name);
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "T").unwrap();
        cfg.set_str("user.email", "t@example.com").unwrap();
        cfg.set_bool("core.autocrlf", false).unwrap();
        commit_file(&repo, &dir, "g.txt", "g", "G");
        let a = commit_file(&repo, &dir, "f.txt", "top\nmiddle\nbottom\n", "A");
        let main = repo.head().unwrap().shorthand().unwrap().to_string();
        repo.branch("other", &repo.find_commit(a).unwrap(), false).unwrap();
        switch(&repo, "other");
        commit_file(&repo, &dir, "f.txt", "top\ntheirs\nbottom\n", "T");
        fs::remove_file(dir.join("g.txt")).unwrap();
        let mut index = repo.index().unwrap();
        index.remove_path(Path::new("g.txt")).unwrap();
        index.write().unwrap();
        {
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let sig = git2::Signature::now("D", "d@example.com").unwrap();
            let parent = repo.head().unwrap().peel_to_commit().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "rm g", &tree, &[&parent]).unwrap();
        }
        switch(&repo, &main);
        commit_file(&repo, &dir, "f.txt", "top\nmine\nbottom\n", "M");
        commit_file(&repo, &dir, "g.txt", "g edited", "M2");
        let p = dir.to_str().unwrap();
        crate::graph::merge::merge(p, "refs/heads/other").unwrap();
        (dir, repo)
    }

    #[test]
    fn a_conflicted_merge_is_left_for_resolution() {
        let (dir, repo) = conflicted_merge("conflict-read");
        let state = merge_state(&repo).unwrap().unwrap();
        assert_eq!(state.conflicts, 2);
        assert!(state.message.starts_with("Merge branch 'other'") && !state.message.contains('#'), "{}", state.message);

        let f = read_conflict(&repo, "f.txt").unwrap();
        assert_eq!((f.ours_exists, f.theirs_exists, f.binary), (true, true, false));
        assert_eq!(f.theirs_label, "other");
        assert_eq!(
            f.segments,
            [
                Segment::Text { text: "top\n".into() },
                Segment::Conflict { ours: "mine\n".into(), theirs: "theirs\n".into() },
                Segment::Text { text: "bottom\n".into() },
            ]
        );
        // Deleted on one side: no line-by-line view, but the sides are known.
        let g = read_conflict(&repo, "g.txt").unwrap();
        assert_eq!((g.ours_exists, g.theirs_exists, g.segments.len()), (true, false, 0));
        assert!(read_conflict(&repo, "nope.txt").is_err());

        assert!(has_unresolved_markers(&repo, "f.txt"));
        assert!(crate::changes::stage(&repo, &["f.txt".to_string()]).unwrap_err().contains("conflict markers"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolving_by_side_and_by_content_lets_the_merge_commit() {
        let (dir, repo) = conflicted_merge("conflict-resolve");
        let before = repo.head().unwrap().peel_to_commit().unwrap().id();

        // Incoming deleted g.txt: taking "theirs" removes it; taking "ours" would keep it.
        resolve_side(&repo, "g.txt", "theirs").unwrap();
        assert!(!dir.join("g.txt").exists());
        assert!(resolve_side(&repo, "g.txt", "theirs").is_err()); // no longer conflicted

        // Hand-written content for f.txt.
        save_resolution(&repo, "f.txt", "top\nmine and theirs\nbottom\n").unwrap();
        let state = merge_state(&repo).unwrap().unwrap();
        assert_eq!(state.conflicts, 0);

        let id = crate::changes::commit_staged(&repo, &state.message).unwrap();
        let merged = repo.find_commit(id).unwrap();
        assert_eq!(merged.parent_count(), 2);
        assert_eq!(merged.parent_id(0).unwrap(), before);
        assert_eq!(repo.state(), RepositoryState::Clean);
        assert_eq!(fs::read_to_string(dir.join("f.txt")).unwrap(), "top\nmine and theirs\nbottom\n");
        assert!(merge_state(&repo).unwrap().is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn taking_our_side_and_aborting() {
        let (dir, repo) = conflicted_merge("conflict-ours");
        let p = dir.to_str().unwrap();
        resolve_side(&repo, "f.txt", "ours").unwrap();
        assert_eq!(fs::read_to_string(dir.join("f.txt")).unwrap(), "top\nmine\nbottom\n");
        assert_eq!(merge_state(&repo).unwrap().unwrap().conflicts, 1);

        abort_merge(p).unwrap();
        assert_eq!(repo.state(), RepositoryState::Clean);
        assert_eq!(fs::read_to_string(dir.join("g.txt")).unwrap(), "g edited");
        assert!(abort_merge(p).unwrap_err().contains("no merge"));
        let _ = fs::remove_dir_all(&dir);
    }
}
