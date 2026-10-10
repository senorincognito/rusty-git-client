//! Staging, unstaging and discarding a single hunk of a file's uncommitted changes, and staging a single line.
//!
//! A hunk is a contiguous run of added/removed lines (see `DiffLine::block`). Instead of building
//! and applying patches, the new contents are rebuilt line by line from the full-file diff:
//! lines keep their exact bytes, so line endings (including CRLF files and a missing final
//! newline) survive. Every request carries the hunk's fingerprint, so a stale diff can never be
//! applied to a different change.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use git2::build::CheckoutBuilder;
use git2::{IndexEntry, IndexTime, Oid, Repository};
use serde::Deserialize;

use crate::commit::{working_diff_raw, FileDiff};

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

const STALE: &str = "The file changed since this diff was shown. The diff has been refreshed; try again.";

fn workdir(repo: &Repository) -> Result<PathBuf, String> {
    repo.workdir().map(Path::to_path_buf).ok_or_else(|| "The repository has no working directory".to_string())
}

/// The unstaged (or, with `staged`, the staged) diff of `path` with its exact line bytes, after
/// checking that every hunk in `blocks` (hunk number and fingerprint) is still the one the caller saw and that the
/// file can be changed hunk by hunk.
fn locate_blocks(
    repo: &Repository,
    path: &str,
    staged: bool,
    blocks: &[(usize, &str)],
) -> Result<(FileDiff, Vec<Vec<u8>>), String> {
    let (diff, raw) = working_diff_raw(repo, path, staged, true)?;
    if diff.binary {
        return Err("Binary files can't be changed hunk by hunk".into());
    }
    if diff.truncated {
        return Err("This diff is too large to change hunk by hunk".into());
    }
    if blocks.iter().any(|(block, id)| diff.blocks.get(*block).map(String::as_str) != Some(*id)) {
        return Err(STALE.into());
    }
    // An untracked file has no index version to build from (a staged new file is fine to unstage).
    if !staged && repo.index().map_err(err)?.get_path(Path::new(path), 0).is_none() {
        return Err(
            "This file is not tracked yet, so it can't be changed hunk by hunk. Stage the whole file instead.".into()
        );
    }
    Ok((diff, raw))
}

fn locate(
    repo: &Repository,
    path: &str,
    staged: bool,
    block: usize,
    id: &str,
) -> Result<(FileDiff, Vec<Vec<u8>>), String> {
    locate_blocks(repo, path, staged, &[(block, id)])
}

/// One changed line as the caller saw it: the `line`-th added or removed line of hunk `block` (counted from 0 within
/// the hunk, the same in the full-file and the changes-only view), with the hunk's fingerprint.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LineRef {
    pub block: usize,
    pub block_id: String,
    pub line: usize,
}

/// What a stage or discard applies to.
enum Pick {
    /// Every changed line of this hunk.
    Hunk(usize),
    /// These `diff.lines` positions.
    Lines(HashSet<usize>),
}

impl Pick {
    fn has(&self, position: usize, line: &crate::commit::DiffLine) -> bool {
        match self {
            Pick::Hunk(block) => line.block == Some(*block),
            Pick::Lines(set) => set.contains(&position),
        }
    }
}

/// Turns line references into positions in `diff.lines`; a reference to a line the hunk doesn't have means the diff is
/// out of date.
fn resolve_lines(diff: &FileDiff, lines: &[(usize, usize)]) -> Result<Pick, String> {
    if lines.is_empty() {
        return Err("No lines selected".into());
    }
    let mut chosen = HashSet::new();
    for (block, n) in lines {
        let position = diff
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.block == Some(*block) && matches!(l.kind, "add" | "del"))
            .nth(*n)
            .map(|(i, _)| i)
            .ok_or(STALE)?;
        chosen.insert(position);
    }
    Ok(Pick::Lines(chosen))
}

/// The distinct hunks named by line references, as (number, fingerprint); one hunk with two fingerprints is stale.
fn hunks_of(lines: &[LineRef]) -> Result<Vec<(usize, &str)>, String> {
    let mut out: Vec<(usize, &str)> = Vec::new();
    for l in lines {
        match out.iter().find(|(b, _)| *b == l.block) {
            Some((_, id)) if *id != l.block_id => return Err(STALE.into()),
            Some(_) => {}
            None => out.push((l.block, l.block_id.as_str())),
        }
    }
    Ok(out)
}

/// Puts hunk `block` of the file's unstaged changes into the index (the staging area), leaving
/// the file on disk and the other hunks untouched.
pub(crate) fn stage_hunk(repo: &Repository, path: &str, block: usize, id: &str) -> Result<(), String> {
    let (diff, raw) = locate(repo, path, false, block, id)?;
    stage_pick(repo, path, &diff, &raw, &Pick::Hunk(block))
}

/// Puts the given changed lines (possibly from several hunks) into the index; the rest of their hunks stays unstaged.
pub(crate) fn stage_lines(repo: &Repository, path: &str, lines: &[LineRef]) -> Result<(), String> {
    let (diff, raw) = locate_blocks(repo, path, false, &hunks_of(lines)?)?;
    let pairs: Vec<(usize, usize)> = lines.iter().map(|l| (l.block, l.line)).collect();
    stage_pick(repo, path, &diff, &raw, &resolve_lines(&diff, &pairs)?)
}

/// Stages what `pick` selects from the unstaged diff.
fn stage_pick(repo: &Repository, path: &str, diff: &FileDiff, raw: &[Vec<u8>], pick: &Pick) -> Result<(), String> {
    let rel = Path::new(path);
    let mut index = repo.index().map_err(err)?;
    let existing = index.get_path(rel, 0).ok_or(STALE)?;

    // The index version, plus only the selected changes: a removed line that is selected drops out, an added one
    // comes in; every other change stays as the index has it.
    let mut out: Vec<u8> = Vec::new();
    for (i, (line, bytes)) in diff.lines.iter().zip(raw).enumerate() {
        let mine = pick.has(i, line);
        match line.kind {
            "ctx" => out.extend_from_slice(bytes),
            "del" if !mine => out.extend_from_slice(bytes),
            "add" if mine => out.extend_from_slice(bytes),
            _ => {}
        }
    }

    if out.is_empty() && !workdir(repo)?.join(rel).exists() {
        // The file was deleted on disk and this hunk is the deletion: stage the removal.
        index.remove_path(rel).map_err(err)?;
    } else {
        let entry = IndexEntry {
            ctime: IndexTime::new(0, 0),
            mtime: IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode: existing.mode,
            uid: 0,
            gid: 0,
            file_size: out.len() as u32,
            id: Oid::ZERO_SHA1,
            flags: 0,
            flags_extended: 0,
            path: existing.path.clone(),
        };
        index.add_frombuffer(&entry, &out).map_err(err)?;
    }
    index.write().map_err(err)
}

/// Takes hunk `block` of the file's staged changes back out of the index: the index keeps the
/// other staged hunks and gets HEAD's lines back for this one. The file on disk is not touched.
pub(crate) fn unstage_hunk(repo: &Repository, path: &str, block: usize, id: &str) -> Result<(), String> {
    let (diff, raw) = locate(repo, path, true, block, id)?;
    let rel = Path::new(path);
    let mut index = repo.index().map_err(err)?;
    let existing = index.get_path(rel, 0);
    // The file as HEAD has it; None for a file that is new (or on a branch without commits).
    let head_mode = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_tree().ok())
        .and_then(|t| t.get_path(rel).ok())
        .map(|e| e.filemode() as u32);

    // Staged result: the staged lines, but this hunk reverted to what HEAD has.
    let mut out: Vec<u8> = Vec::new();
    for (line, bytes) in diff.lines.iter().zip(&raw) {
        let mine = line.block == Some(block);
        match line.kind {
            "ctx" => out.extend_from_slice(bytes),
            "add" if !mine => out.extend_from_slice(bytes),
            "del" if mine => out.extend_from_slice(bytes),
            _ => {}
        }
    }

    if out.is_empty() && head_mode.is_none() {
        // A newly added file: unstaging its (only) hunk removes it from the index again.
        index.remove_path(rel).map_err(err)?;
    } else {
        let mode = existing.as_ref().map(|e| e.mode).or(head_mode).ok_or(STALE)?;
        let entry = IndexEntry {
            ctime: IndexTime::new(0, 0),
            mtime: IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode,
            uid: 0,
            gid: 0,
            file_size: out.len() as u32,
            id: Oid::ZERO_SHA1,
            flags: 0,
            flags_extended: 0,
            path: existing.map_or_else(|| path.as_bytes().to_vec(), |e| e.path),
        };
        index.add_frombuffer(&entry, &out).map_err(err)?;
    }
    index.write().map_err(err)
}

/// Splits bytes into lines, each keeping its own terminator ("\n" or "\r\n").
fn split_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            lines.push(&bytes[start..=i]);
            start = i + 1;
        }
    }
    if start < bytes.len() {
        lines.push(&bytes[start..]);
    }
    lines
}

/// True when most of the file's line endings are CRLF.
fn mostly_crlf(bytes: &[u8]) -> bool {
    let crlf = bytes.windows(2).filter(|w| w == b"\r\n").count();
    let lf = bytes.iter().filter(|b| **b == b'\n').count();
    crlf * 2 > lf
}

/// Restored lines come from the index (always "\n"); give them the file's own line ending.
fn with_eol(line: &[u8], crlf: bool) -> Vec<u8> {
    let mut out = line.to_vec();
    if crlf && out.ends_with(b"\n") && !out.ends_with(b"\r\n") {
        out.pop();
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// Throws away hunk `block` of the file's unstaged changes: the file on disk gets the index version
/// of those lines back, the other changes stay. Undo (the journal) can bring it back.
pub(crate) fn discard_hunk(repo: &Repository, path: &str, block: usize, id: &str) -> Result<(), String> {
    let (diff, raw) = locate(repo, path, false, block, id)?;
    let full = workdir(repo)?.join(path);
    if !full.exists() {
        // Deleted on disk: discarding the deletion brings the file back from the index.
        let mut index = repo.index().map_err(err)?;
        let mut checkout = CheckoutBuilder::new();
        checkout.force().path(path);
        return repo.checkout_index(Some(&mut index), Some(&mut checkout)).map_err(err);
    }
    discard_pick(repo, path, &diff, &raw, &Pick::Hunk(block))
}

/// Throws away the given changed lines (possibly from several hunks): an added line is taken out of the file, a
/// removed one is put back where it was.
pub(crate) fn discard_lines(repo: &Repository, path: &str, lines: &[LineRef]) -> Result<(), String> {
    let (diff, raw) = locate_blocks(repo, path, false, &hunks_of(lines)?)?;
    let pairs: Vec<(usize, usize)> = lines.iter().map(|l| (l.block, l.line)).collect();
    discard_pick(repo, path, &diff, &raw, &resolve_lines(&diff, &pairs)?)
}

/// Rewrites the file on disk without what `pick` selects from the unstaged diff.
fn discard_pick(repo: &Repository, path: &str, diff: &FileDiff, raw: &[Vec<u8>], pick: &Pick) -> Result<(), String> {
    let full = workdir(repo)?.join(Path::new(path));

    // A deleted file has no lines on disk; restoring one of its lines creates the file with just that line.
    let bytes = if full.exists() { std::fs::read(&full).map_err(|e| e.to_string())? } else { Vec::new() };
    let lines = split_lines(&bytes);
    let in_file = diff.lines.iter().filter(|l| l.kind == "ctx" || l.kind == "add").count();
    if lines.len() != in_file {
        return Err(STALE.into());
    }
    let crlf = mostly_crlf(&bytes);

    let mut out: Vec<u8> = Vec::new();
    let mut at = 0; // position in the file on disk
    for (i, (line, old)) in diff.lines.iter().zip(raw).enumerate() {
        let mine = pick.has(i, line);
        match line.kind {
            "ctx" => {
                out.extend_from_slice(lines[at]);
                at += 1;
            }
            "add" => {
                if !mine {
                    out.extend_from_slice(lines[at]);
                }
                at += 1;
            }
            "del" if mine => out.extend_from_slice(&with_eol(old, crlf)),
            _ => {}
        }
    }
    std::fs::write(&full, out).map_err(|e| e.to_string())
}

async fn blocking(
    path: String,
    f: impl FnOnce(&Repository) -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || f(&Repository::discover(&path).map_err(err)?))
        .await
        .map_err(|e| e.to_string())?
}

/// Stages one hunk of a file's unstaged changes. `block_id` is the hunk's fingerprint from the diff.
#[tauri::command]
pub async fn stage_hunk_cmd(path: String, file: String, block: usize, block_id: String) -> Result<(), String> {
    let label = format!("Stage a hunk of {}", crate::undo::describe_paths(std::slice::from_ref(&file)));
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Index, || {
        blocking(path, move |r| stage_hunk(r, &file, block, &block_id))
    })
    .await
}

/// Stages the given changed lines of a file's unstaged changes (a single line, or a selection spanning hunks).
#[tauri::command]
pub async fn stage_lines_cmd(path: String, file: String, lines: Vec<LineRef>) -> Result<(), String> {
    let name = crate::undo::describe_paths(std::slice::from_ref(&file));
    let label = if lines.len() == 1 {
        format!("Stage a line of {name}")
    } else {
        format!("Stage {} lines of {name}", lines.len())
    };
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Index, || {
        blocking(path, move |r| stage_lines(r, &file, &lines))
    })
    .await
}

/// Takes one hunk of a file's staged changes out of the staging area again.
#[tauri::command]
pub async fn unstage_hunk_cmd(path: String, file: String, block: usize, block_id: String) -> Result<(), String> {
    let label = format!("Unstage a hunk of {}", crate::undo::describe_paths(std::slice::from_ref(&file)));
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Index, || {
        blocking(path, move |r| unstage_hunk(r, &file, block, &block_id))
    })
    .await
}

/// Discards the given changed lines of a file's unstaged changes (see [`discard_lines`]).
#[tauri::command]
pub async fn discard_lines_cmd(path: String, file: String, lines: Vec<LineRef>) -> Result<(), String> {
    let name = crate::undo::describe_paths(std::slice::from_ref(&file));
    let label = if lines.len() == 1 {
        format!("Discard a line of {name}")
    } else {
        format!("Discard {} lines of {name}", lines.len())
    };
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Full, || {
        blocking(path, move |r| discard_lines(r, &file, &lines))
    })
    .await
}

/// Discards one hunk of a file's unstaged changes (undoable through the journal).
#[tauri::command]
pub async fn discard_hunk_cmd(path: String, file: String, block: usize, block_id: String) -> Result<(), String> {
    let label = format!("Discard a hunk of {}", crate::undo::describe_paths(std::slice::from_ref(&file)));
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Full, || {
        blocking(path, move |r| discard_hunk(r, &file, block, &block_id))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::working_diff_raw as diff_of;
    use git2::Signature;
    use std::fs;

    fn setup(name: &str) -> (PathBuf, Repository) {
        let dir = std::env::temp_dir().join(format!("gc-hunks-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "T").unwrap();
        cfg.set_str("user.email", "t@example.com").unwrap();
        cfg.set_str("core.autocrlf", "false").unwrap(); // the result must not depend on the machine's git config
        (dir, repo)
    }

    fn line(block: usize, id: &str, line: usize) -> LineRef {
        LineRef { block, block_id: id.to_string(), line }
    }
    fn stage_line(repo: &Repository, p: &str, block: usize, id: &str, n: usize) -> Result<(), String> {
        stage_lines(repo, p, &[line(block, id, n)])
    }
    fn discard_line(repo: &Repository, p: &str, block: usize, id: &str, n: usize) -> Result<(), String> {
        discard_lines(repo, p, &[line(block, id, n)])
    }

    fn commit_paths(repo: &Repository, paths: &[&str], msg: &str) {
        let mut index = repo.index().unwrap();
        for p in paths {
            index.add_path(Path::new(p)).unwrap();
        }
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("T", "t@example.com").unwrap();
        let parents: Vec<_> = repo.head().ok().and_then(|h| h.peel_to_commit().ok()).into_iter().collect();
        let refs: Vec<_> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &refs).unwrap();
    }

    fn lines(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("line {i}")).collect()
    }
    fn text(v: &[String]) -> String {
        v.join("\n") + "\n"
    }
    fn unstaged(repo: &Repository, p: &str) -> FileDiff {
        diff_of(repo, p, false, true).unwrap().0
    }
    fn staged(repo: &Repository, p: &str) -> FileDiff {
        diff_of(repo, p, true, true).unwrap().0
    }
    fn index_text(repo: &Repository, p: &str) -> String {
        let idx = repo.index().unwrap();
        let entry = idx.get_path(Path::new(p), 0).unwrap();
        String::from_utf8(repo.find_blob(entry.id).unwrap().content().to_vec()).unwrap()
    }

    #[test]
    fn stage_and_discard_single_hunks() {
        let (dir, repo) = setup("basic");
        fs::write(dir.join("a.txt"), text(&lines(30))).unwrap();
        commit_paths(&repo, &["a.txt"], "base");

        // Four separate hunks: two edits, a deletion and an addition at the end.
        let mut v = lines(30);
        v[2] = "line 3 CHANGED".into();
        v[11] = "line 12 CHANGED".into();
        v.remove(20);
        v.push("line 31".into());
        fs::write(dir.join("a.txt"), text(&v)).unwrap();
        let d = unstaged(&repo, "a.txt");
        assert_eq!(d.blocks.len(), 4);
        assert_eq!(d.lines.iter().filter(|l| l.block == Some(1)).count(), 2, "one removed + one added line");

        // A stale fingerprint is refused.
        assert!(stage_hunk(&repo, "a.txt", 1, "0000000000000000").unwrap_err().contains("changed since"));
        assert!(stage_hunk(&repo, "a.txt", 9, &d.blocks[0]).is_err());

        // Stage hunk 1 only: the index gets just that edit, the file on disk is untouched.
        stage_hunk(&repo, "a.txt", 1, &d.blocks[1]).unwrap();
        let mut only_12 = lines(30);
        only_12[11] = "line 12 CHANGED".into();
        assert_eq!(index_text(&repo, "a.txt"), text(&only_12));
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), text(&v));
        let (un, st) = (unstaged(&repo, "a.txt"), staged(&repo, "a.txt"));
        assert_eq!((un.blocks.len(), st.blocks.len()), (3, 1));
        assert_eq!((st.additions, st.deletions), (1, 1));

        // Discard the first remaining hunk (the line-3 edit): only that change disappears from disk.
        discard_hunk(&repo, "a.txt", 0, &un.blocks[0]).unwrap();
        let mut expected = v.clone();
        expected[2] = "line 3".into();
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), text(&expected));
        let un = unstaged(&repo, "a.txt");
        assert_eq!(un.blocks.len(), 2);
        assert_eq!(index_text(&repo, "a.txt"), text(&only_12), "discarding never touches the index");

        // Stage the appended line (the last hunk).
        stage_hunk(&repo, "a.txt", 1, &un.blocks[1]).unwrap();
        assert!(index_text(&repo, "a.txt").ends_with("line 31\n"));
        assert_eq!(unstaged(&repo, "a.txt").blocks.len(), 1, "only the deletion of line 21 is left");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn crlf_files_keep_their_line_endings() {
        let (dir, repo) = setup("crlf");
        repo.config().unwrap().set_str("core.autocrlf", "true").unwrap();
        fs::write(dir.join("w.txt"), text(&lines(20))).unwrap(); // LF in the repository
        commit_paths(&repo, &["w.txt"], "base");

        // On disk the file has CRLF endings and two edits.
        let mut v = lines(20);
        v[1] = "line 2 CHANGED".into();
        v[15] = "line 16 CHANGED".into();
        fs::write(dir.join("w.txt"), text(&v).replace('\n', "\r\n")).unwrap();
        let d = unstaged(&repo, "w.txt");
        assert_eq!(d.blocks.len(), 2, "line-ending conversion alone is not a change");

        // Staging stores LF (as git would); the file on disk keeps CRLF.
        stage_hunk(&repo, "w.txt", 0, &d.blocks[0]).unwrap();
        let mut staged_text = lines(20);
        staged_text[1] = "line 2 CHANGED".into();
        assert_eq!(index_text(&repo, "w.txt"), text(&staged_text));
        assert_eq!(fs::read_to_string(dir.join("w.txt")).unwrap(), text(&v).replace('\n', "\r\n"));

        // Discarding restores the old line with the file's own CRLF ending.
        let d = unstaged(&repo, "w.txt");
        discard_hunk(&repo, "w.txt", 0, &d.blocks[0]).unwrap();
        let mut expected = v.clone();
        expected[15] = "line 16".into();
        assert_eq!(fs::read_to_string(dir.join("w.txt")).unwrap(), text(&expected).replace('\n', "\r\n"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_final_newline_is_preserved() {
        let (dir, repo) = setup("eof");
        fs::write(dir.join("n.txt"), "a\nb").unwrap(); // no newline at the end
        commit_paths(&repo, &["n.txt"], "base");

        fs::write(dir.join("n.txt"), "a\nb\nc").unwrap();
        let d = unstaged(&repo, "n.txt");
        assert_eq!(d.blocks.len(), 1);
        stage_hunk(&repo, "n.txt", 0, &d.blocks[0]).unwrap();
        assert_eq!(index_text(&repo, "n.txt"), "a\nb\nc");

        // Back to the committed state: discarding the same kind of change restores the exact bytes.
        fs::write(dir.join("n.txt"), "a\nb\nc\nd").unwrap();
        let d = unstaged(&repo, "n.txt");
        discard_hunk(&repo, "n.txt", 0, &d.blocks[0]).unwrap();
        assert_eq!(fs::read_to_string(dir.join("n.txt")).unwrap(), "a\nb\nc");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn deleted_untracked_and_binary_files() {
        let (dir, repo) = setup("special");
        fs::write(dir.join("gone.txt"), text(&lines(5))).unwrap();
        fs::write(dir.join("bin.dat"), [0u8, 1, 2, 0, 255]).unwrap();
        commit_paths(&repo, &["gone.txt", "bin.dat"], "base");

        // Deleted on disk: staging the hunk stages the removal, discarding brings the file back.
        fs::remove_file(dir.join("gone.txt")).unwrap();
        let d = unstaged(&repo, "gone.txt");
        assert_eq!(d.blocks.len(), 1);
        discard_hunk(&repo, "gone.txt", 0, &d.blocks[0]).unwrap();
        assert_eq!(fs::read_to_string(dir.join("gone.txt")).unwrap(), text(&lines(5)));
        fs::remove_file(dir.join("gone.txt")).unwrap();
        let d = unstaged(&repo, "gone.txt");
        stage_hunk(&repo, "gone.txt", 0, &d.blocks[0]).unwrap();
        assert!(repo.index().unwrap().get_path(Path::new("gone.txt"), 0).is_none());

        // An untracked file has no index version to build from.
        fs::write(dir.join("new.txt"), "x\n").unwrap();
        let d = unstaged(&repo, "new.txt");
        assert!(stage_hunk(&repo, "new.txt", 0, &d.blocks[0]).unwrap_err().contains("not tracked"));
        assert!(discard_hunk(&repo, "new.txt", 0, &d.blocks[0]).unwrap_err().contains("not tracked"));
        assert!(dir.join("new.txt").exists());

        // Binary files are refused.
        fs::write(dir.join("bin.dat"), [9u8, 0, 0, 0, 1]).unwrap();
        assert!(stage_hunk(&repo, "bin.dat", 0, "x").unwrap_err().contains("Binary"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unstaging_single_hunks() {
        let (dir, repo) = setup("unstage");
        fs::write(dir.join("a.txt"), text(&lines(30))).unwrap();
        commit_paths(&repo, &["a.txt"], "base");

        // Edit in four places and stage the whole file.
        let mut v = lines(30);
        v[2] = "line 3 CHANGED".into();
        v[11] = "line 12 CHANGED".into();
        v.remove(20);
        v.push("line 31".into());
        fs::write(dir.join("a.txt"), text(&v)).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let d = staged(&repo, "a.txt");
        assert_eq!(d.blocks.len(), 4);
        assert!(unstage_hunk(&repo, "a.txt", 1, "0000000000000000").unwrap_err().contains("changed since"));

        // Unstage hunk 1 (the line-12 edit): the index loses just that one, the file on disk keeps it.
        unstage_hunk(&repo, "a.txt", 1, &d.blocks[1]).unwrap();
        let mut expected = v.clone();
        expected[11] = "line 12".into();
        assert_eq!(index_text(&repo, "a.txt"), text(&expected));
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), text(&v));
        assert_eq!((staged(&repo, "a.txt").blocks.len(), unstaged(&repo, "a.txt").blocks.len()), (3, 1));

        // Unstage the rest one by one: the index ends up equal to HEAD again.
        for _ in 0..3 {
            let d = staged(&repo, "a.txt");
            unstage_hunk(&repo, "a.txt", 0, &d.blocks[0]).unwrap();
        }
        assert_eq!(index_text(&repo, "a.txt"), text(&lines(30)));
        assert!(staged(&repo, "a.txt").blocks.is_empty());
        assert_eq!(unstaged(&repo, "a.txt").blocks.len(), 4, "everything is back in the working changes");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unstaging_new_deleted_and_first_commit_files() {
        let (dir, repo) = setup("unstage-special");

        // On a branch without commits: a staged file unstages to nothing.
        fs::write(dir.join("first.txt"), "one\ntwo\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("first.txt")).unwrap();
        index.write().unwrap();
        let d = staged(&repo, "first.txt");
        assert_eq!(d.blocks.len(), 1);
        unstage_hunk(&repo, "first.txt", 0, &d.blocks[0]).unwrap();
        assert!(repo.index().unwrap().get_path(Path::new("first.txt"), 0).is_none());

        // Normal history from here on.
        fs::write(dir.join("keep.txt"), text(&lines(5))).unwrap();
        fs::write(dir.join("eof.txt"), "a\nb").unwrap(); // no final newline
        commit_paths(&repo, &["keep.txt", "eof.txt"], "base");

        // A newly added file (not in HEAD) is removed from the index again.
        fs::write(dir.join("new.txt"), "x\ny\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("new.txt")).unwrap();
        index.write().unwrap();
        let d = staged(&repo, "new.txt");
        unstage_hunk(&repo, "new.txt", 0, &d.blocks[0]).unwrap();
        assert!(repo.index().unwrap().get_path(Path::new("new.txt"), 0).is_none());
        assert!(dir.join("new.txt").exists(), "the file itself stays");

        // A staged deletion comes back into the index.
        fs::remove_file(dir.join("keep.txt")).unwrap();
        let mut index = repo.index().unwrap();
        index.remove_path(Path::new("keep.txt")).unwrap();
        index.write().unwrap();
        let d = staged(&repo, "keep.txt");
        assert_eq!(d.blocks.len(), 1);
        unstage_hunk(&repo, "keep.txt", 0, &d.blocks[0]).unwrap();
        assert_eq!(index_text(&repo, "keep.txt"), text(&lines(5)));
        assert!(!dir.join("keep.txt").exists(), "unstaging never touches the file on disk");

        // The missing final newline of HEAD's version is restored exactly.
        fs::write(dir.join("eof.txt"), "a\nb\nc\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("eof.txt")).unwrap();
        index.write().unwrap();
        let d = staged(&repo, "eof.txt");
        unstage_hunk(&repo, "eof.txt", 0, &d.blocks[0]).unwrap();
        assert_eq!(index_text(&repo, "eof.txt"), "a\nb");

        // Binary files are refused.
        fs::write(dir.join("bin.dat"), [0u8, 1, 2]).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("bin.dat")).unwrap();
        index.write().unwrap();
        assert!(unstage_hunk(&repo, "bin.dat", 0, "x").unwrap_err().contains("Binary"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stages_single_lines_of_a_hunk() {
        let (dir, repo) = setup("line");
        fs::write(dir.join("a.txt"), "a\nb\nc\nd\ne\n").unwrap();
        commit_paths(&repo, &["a.txt"], "base");
        // One hunk replaces b with B and c with C: lines of the hunk are -b, -c, +B, +C.
        fs::write(dir.join("a.txt"), "a\nB\nC\nd\ne\n").unwrap();
        let d = unstaged(&repo, "a.txt");
        assert_eq!(d.blocks.len(), 1);
        let kinds: Vec<_> = d.lines.iter().filter(|l| l.block == Some(0)).map(|l| (l.kind, l.text.as_str())).collect();
        assert_eq!(kinds, [("del", "b"), ("del", "c"), ("add", "B"), ("add", "C")]);

        // Only the added line B: the index keeps b and c and gains B.
        stage_line(&repo, "a.txt", 0, &d.blocks[0], 2).unwrap();
        assert_eq!(index_text(&repo, "a.txt"), "a\nb\nc\nB\nd\ne\n");
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "a\nB\nC\nd\ne\n"); // the file is untouched

        // The old fingerprint no longer matches: nothing is applied on a stale diff.
        assert!(stage_line(&repo, "a.txt", 0, &d.blocks[0], 0).unwrap_err().contains("changed since"));

        // The refreshed diff: now the removed b (offset 0) can be staged, leaving c and C for later.
        let d = unstaged(&repo, "a.txt");
        // The context line B now separates the removals (hunk 0) from the added C (hunk 1).
        assert_eq!(d.blocks.len(), 2);
        let first: Vec<_> = d.lines.iter().filter(|l| l.block == Some(0)).map(|l| (l.kind, l.text.as_str())).collect();
        assert_eq!(first, [("del", "b"), ("del", "c")]);
        stage_line(&repo, "a.txt", 0, &d.blocks[0], 0).unwrap();
        assert_eq!(index_text(&repo, "a.txt"), "a\nc\nB\nd\ne\n");

        // An offset past the hunk's last changed line is refused.
        let d = unstaged(&repo, "a.txt");
        assert!(stage_line(&repo, "a.txt", 0, &d.blocks[0], 9).unwrap_err().contains("changed since"));

        // Staging the rest one line at a time ends with the file fully staged.
        loop {
            let d = unstaged(&repo, "a.txt");
            if d.blocks.is_empty() {
                break;
            }
            stage_line(&repo, "a.txt", 0, &d.blocks[0], 0).unwrap();
        }
        assert_eq!(index_text(&repo, "a.txt"), "a\nB\nC\nd\ne\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn staging_a_line_keeps_crlf_endings() {
        let (dir, repo) = setup("line-crlf");
        fs::write(dir.join("a.txt"), "one\r\ntwo\r\nthree\r\n").unwrap();
        commit_paths(&repo, &["a.txt"], "base");
        fs::write(dir.join("a.txt"), "one\r\nTWO\r\nthree\r\nfour\r\n").unwrap();
        let d = unstaged(&repo, "a.txt");
        // Blocks: -two +TWO (0) and +four (1). Stage "+four" only.
        assert_eq!(d.blocks.len(), 2);
        stage_line(&repo, "a.txt", 1, &d.blocks[1], 0).unwrap();
        assert_eq!(index_text(&repo, "a.txt"), "one\r\ntwo\r\nthree\r\nfour\r\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn discards_single_lines_of_a_hunk() {
        let (dir, repo) = setup("discard-line");
        fs::write(dir.join("a.txt"), "a\nb\nc\nd\ne\n").unwrap();
        commit_paths(&repo, &["a.txt"], "base");
        fs::write(dir.join("a.txt"), "a\nB\nC\nd\ne\n").unwrap();
        let on_disk = || fs::read_to_string(dir.join("a.txt")).unwrap();
        let d = unstaged(&repo, "a.txt");
        // Hunk 0 is -b -c +B +C. Discarding the added B takes it out of the file.
        discard_line(&repo, "a.txt", 0, &d.blocks[0], 2).unwrap();
        assert_eq!(on_disk(), "a\nC\nd\ne\n");
        // The index is never touched.
        assert_eq!(index_text(&repo, "a.txt"), "a\nb\nc\nd\ne\n");

        // The old fingerprint is stale now.
        assert!(discard_line(&repo, "a.txt", 0, &d.blocks[0], 0).unwrap_err().contains("changed since"));

        // Discarding the removed b puts it back in its place.
        let d = unstaged(&repo, "a.txt");
        discard_line(&repo, "a.txt", 0, &d.blocks[0], 0).unwrap();
        assert_eq!(on_disk(), "a\nb\nC\nd\ne\n");

        // An offset past the hunk is refused; the rest is discarded line by line until nothing differs.
        let d = unstaged(&repo, "a.txt");
        assert!(discard_line(&repo, "a.txt", 0, &d.blocks[0], 7).unwrap_err().contains("changed since"));
        loop {
            let d = unstaged(&repo, "a.txt");
            if d.blocks.is_empty() {
                break;
            }
            discard_line(&repo, "a.txt", 0, &d.blocks[0], 0).unwrap();
        }
        assert_eq!(on_disk(), "a\nb\nc\nd\ne\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_discarded_line_gets_the_files_line_ending_back() {
        let (dir, repo) = setup("discard-line-crlf");
        fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").unwrap();
        commit_paths(&repo, &["a.txt"], "base");
        // The file on disk uses CRLF; "two" is removed there.
        fs::write(dir.join("a.txt"), "one\r\nthree\r\nfour\r\n").unwrap();
        let d = unstaged(&repo, "a.txt");
        // Every line differs in its ending, so the removed "two" is not the first changed line of its hunk.
        let at = d.lines.iter().position(|l| l.kind == "del" && l.text == "two").unwrap();
        let block = d.lines[at].block.unwrap();
        let offset = d.lines[..at].iter().filter(|l| l.block == Some(block) && matches!(l.kind, "add" | "del")).count();
        discard_line(&repo, "a.txt", block, &d.blocks[block], offset).unwrap();
        let text = fs::read_to_string(dir.join("a.txt")).unwrap();
        assert!(text.contains("two\r\n"), "{text:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn several_lines_across_hunks_are_staged_and_discarded_together() {
        let (dir, repo) = setup("multi");
        fs::write(dir.join("a.txt"), text(&lines(20))).unwrap();
        commit_paths(&repo, &["a.txt"], "base");
        // Two separate hunks: line 3 -> THREE and line 15 -> FIFTEEN, plus a new last line.
        let mut v = lines(20);
        v[2] = "THREE".into();
        v[14] = "FIFTEEN".into();
        v.push("line 21".into());
        fs::write(dir.join("a.txt"), text(&v)).unwrap();
        let d = unstaged(&repo, "a.txt");
        assert_eq!(d.blocks.len(), 3);
        let id = |b: usize| d.blocks[b].clone();

        // Stage the added THREE (hunk 0, second changed line) and the new last line (hunk 2) in one go.
        stage_lines(&repo, "a.txt", &[line(0, &id(0), 1), line(2, &id(2), 0)]).unwrap();
        let mut want = lines(20);
        want.insert(3, "THREE".into()); // line 3 is still in the index, THREE is added after it
        want.push("line 21".into());
        assert_eq!(index_text(&repo, "a.txt"), text(&want));

        // A selection naming one hunk with two different fingerprints is refused.
        assert!(stage_lines(&repo, "a.txt", &[line(0, "x", 0)]).unwrap_err().contains("changed since"));
        assert!(stage_lines(&repo, "a.txt", &[]).unwrap_err().contains("No lines"));

        // Discard the added FIFTEEN and the removed "line 3" (hunk 0 offset 0) together.
        let d = unstaged(&repo, "a.txt");
        let find = |text: &str| {
            let at = d.lines.iter().position(|l| l.text == text && matches!(l.kind, "add" | "del")).unwrap();
            let block = d.lines[at].block.unwrap();
            let n = d.lines[..at].iter().filter(|l| l.block == Some(block) && matches!(l.kind, "add" | "del")).count();
            line(block, &d.blocks[block], n)
        };
        discard_lines(&repo, "a.txt", &[find("FIFTEEN"), find("line 3")]).unwrap();
        let disk = fs::read_to_string(dir.join("a.txt")).unwrap();
        // FIFTEEN is gone and "line 3" is back next to the staged THREE; the removed "line 15" was not selected.
        assert!(!disk.contains("FIFTEEN") && disk.contains("line 3\n") && disk.contains("THREE\n"));
        assert!(!disk.contains("line 15\n"));
        let _ = fs::remove_dir_all(&dir);
    }
}
