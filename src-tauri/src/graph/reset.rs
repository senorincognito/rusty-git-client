use std::collections::HashSet;

use git2::{BranchType, Commit, Oid, Repository, RepositoryState, ResetType, Sort};
use serde::Serialize;

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

/// How many summaries of the removed commits are listed in the confirmation.
const LISTED: usize = 5;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ResetInfo {
    /// The checked-out branch; None on a detached HEAD.
    pub branch: Option<String>,
    pub from_short: String,
    pub target_short: String,
    pub target_summary: String,
    /// The target is the commit HEAD already points at.
    pub same_commit: bool,
    /// The target is HEAD or one of its ancestors (the branch moves back).
    pub is_ancestor: bool,
    /// Commits that are on the branch now but not reachable from the target afterwards.
    pub removed: usize,
    /// The newest few of them (summaries, newest first).
    pub removed_summaries: Vec<String>,
    /// Commits the branch gains (reachable from the target, not from HEAD now).
    pub added: usize,
    /// How many of the removed commits are already on the upstream (a force push is then needed).
    pub pushed_removed: usize,
    /// Files with uncommitted changes (a hard reset throws these away).
    pub working_changes: usize,
}

/// HEAD's commit and the target, after the checks every reset needs.
fn prepare<'r>(repo: &'r Repository, id: &str) -> Result<(Commit<'r>, Commit<'r>), String> {
    if repo.state() != RepositoryState::Clean {
        return Err("Finish the merge, rebase or other operation in progress first".into());
    }
    let head = repo.head().and_then(|h| h.peel_to_commit()).map_err(|_| "There are no commits yet".to_string())?;
    let oid = Oid::from_str(id).map_err(err)?;
    let target = repo.find_commit(oid).map_err(err)?;
    if crate::sidebar::stash::stash_index_of(repo, oid).is_some() {
        return Err("A stash can't be used as a reset target".into());
    }
    Ok((head, target))
}

/// Commits reachable from `include` but not from `exclude`, newest first.
fn walk(repo: &Repository, include: Oid, exclude: Oid) -> Result<Vec<Oid>, String> {
    let mut w = repo.revwalk().map_err(err)?;
    w.set_sorting(Sort::TOPOLOGICAL | Sort::TIME).map_err(err)?;
    w.push(include).map_err(err)?;
    w.hide(exclude).map_err(err)?;
    w.collect::<Result<Vec<_>, _>>().map_err(err)
}

/// The tip of the checked-out branch's upstream, if it has one.
fn upstream_tip(repo: &Repository) -> Option<Oid> {
    let head = repo.head().ok()?;
    if !head.is_branch() {
        return None;
    }
    let local = repo.find_branch(head.shorthand().ok()?, BranchType::Local).ok()?;
    local.upstream().ok()?.get().target()
}

fn reset_info(repo: &Repository, id: &str) -> Result<ResetInfo, String> {
    let (head, target) = prepare(repo, id)?;
    let removed = walk(repo, head.id(), target.id())?;
    let added = walk(repo, target.id(), head.id())?;

    let pushed_removed = match upstream_tip(repo) {
        Some(up) if !removed.is_empty() => {
            let removed_set: HashSet<Oid> = removed.iter().copied().collect();
            walk(repo, up, target.id())?.iter().filter(|o| removed_set.contains(o)).count()
        }
        _ => 0,
    };
    let summary = |oid: &Oid| -> String {
        repo.find_commit(*oid).ok().and_then(|c| c.summary().ok().flatten().map(str::to_string)).unwrap_or_default()
    };
    let head_ref = repo.head().map_err(err)?;

    Ok(ResetInfo {
        branch: if head_ref.is_branch() { head_ref.shorthand().ok().map(str::to_string) } else { None },
        from_short: head.id().to_string()[..7].to_string(),
        target_short: target.id().to_string()[..7].to_string(),
        target_summary: target.summary().ok().flatten().unwrap_or("").to_string(),
        same_commit: head.id() == target.id(),
        is_ancestor: head.id() == target.id() || repo.graph_descendant_of(head.id(), target.id()).unwrap_or(false),
        removed: removed.len(),
        removed_summaries: removed.iter().take(LISTED).map(summary).collect(),
        added: added.len(),
        pushed_removed,
        working_changes: crate::changes::status_of(repo)?.len(),
    })
}

/// Moves the checked-out branch (or a detached HEAD) to `id`:
/// - "soft": only the branch moves; the staging area and the files stay as they are,
/// - "mixed": the staging area is reset to that commit too; the files stay,
/// - "hard": the staging area and the files are reset as well (uncommitted changes are lost).
fn reset_to(repo: &Repository, id: &str, mode: &str) -> Result<(), String> {
    let kind = match mode {
        "soft" => ResetType::Soft,
        "mixed" => ResetType::Mixed,
        "hard" => ResetType::Hard,
        other => return Err(format!("Unknown reset mode \"{other}\"")),
    };
    let (_, target) = prepare(repo, id)?;
    repo.reset(target.as_object(), kind, None).map_err(err)
}

async fn blocking<T: Send + 'static>(
    path: String,
    f: impl FnOnce(&Repository) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || f(&Repository::discover(&path).map_err(err)?))
        .await
        .map_err(|e| e.to_string())?
}

/// What resetting to a commit would do, for the confirmation dialog.
#[tauri::command]
pub async fn get_reset_info(path: String, id: String) -> Result<ResetInfo, String> {
    blocking(path, move |r| reset_info(r, &id)).await
}

/// `git reset --soft`, `--mixed` or `--hard` to a commit.
#[tauri::command]
pub async fn reset_to_commit(path: String, id: String, mode: String) -> Result<(), String> {
    // Soft keeps the index, mixed rewrites it, hard rewrites the files as well.
    let kind = match mode.as_str() {
        "soft" => crate::undo::Kind::Keep,
        "mixed" => crate::undo::Kind::Index,
        _ => crate::undo::Kind::Full,
    };
    let label = format!("Reset ({mode}) to {}", crate::undo::short_ref(&id));
    crate::undo::recorded(&path.clone(), label, kind, || blocking(path, move |r| reset_to(r, &id, &mode))).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Signature, Status, StatusOptions};
    use std::fs;
    use std::path::{Path, PathBuf};

    /// Writes the files, stages them and commits on HEAD.
    fn commit_files(repo: &Repository, dir: &Path, files: &[(&str, &str)], msg: &str) -> Oid {
        let mut index = repo.index().unwrap();
        for (name, content) in files {
            fs::write(dir.join(name), content).unwrap();
            index.add_path(Path::new(name)).unwrap();
        }
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("R", "r@example.com").unwrap();
        let parents: Vec<_> = repo.head().ok().and_then(|h| h.peel_to_commit().ok()).into_iter().collect();
        let refs: Vec<_> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &refs).unwrap()
    }

    /// a: a.txt "1"    b: a.txt "2", b.txt    c: c.txt    (HEAD = c)
    fn history(name: &str) -> (PathBuf, Repository, Oid, Oid, Oid) {
        let dir = std::env::temp_dir().join(format!("gc-reset-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "R").unwrap();
        cfg.set_str("user.email", "r@example.com").unwrap();
        cfg.set_str("core.autocrlf", "false").unwrap(); // the result must not depend on the machine's git config
        let a = commit_files(&repo, &dir, &[("a.txt", "1")], "a");
        let b = commit_files(&repo, &dir, &[("a.txt", "2"), ("b.txt", "bee")], "b");
        let c = commit_files(&repo, &dir, &[("c.txt", "sea")], "c");
        (dir, repo, a, b, c)
    }

    fn status(repo: &Repository) -> Vec<(String, Status)> {
        let mut o = StatusOptions::new();
        o.include_untracked(true).recurse_untracked_dirs(true);
        let mut v: Vec<_> =
            repo.statuses(Some(&mut o)).unwrap().iter().map(|e| (e.path().unwrap().to_string(), e.status())).collect();
        v.sort_by(|x, y| x.0.cmp(&y.0));
        v
    }

    fn has(st: &[(String, Status)], path: &str, flag: Status) -> bool {
        st.iter().any(|(p, s)| p == path && s.contains(flag))
    }

    #[test]
    fn the_three_modes_move_the_branch_but_treat_files_differently() {
        let (dir, repo, a, _b, c) = history("modes");

        // The info describes what will happen.
        let info = reset_info(&repo, &a.to_string()).unwrap();
        assert_eq!((info.removed, info.added, info.same_commit, info.is_ancestor), (2, 0, false, true));
        assert_eq!(info.removed_summaries, ["c", "b"]);
        assert_eq!((info.target_summary.as_str(), info.working_changes, info.pushed_removed), ("a", 0, 0));
        assert!(info.branch.is_some());

        // Soft: only the branch moves; the removed commits' changes show up as staged.
        reset_to(&repo, &a.to_string(), "soft").unwrap();
        assert_eq!(repo.head().unwrap().target(), Some(a));
        let st = status(&repo);
        assert!(has(&st, "a.txt", Status::INDEX_MODIFIED), "{st:?}");
        assert!(has(&st, "c.txt", Status::INDEX_NEW), "{st:?}");
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "2");

        // Moving forward again: the old tip is no longer an ancestor, and the info counts what is gained.
        let info = reset_info(&repo, &c.to_string()).unwrap();
        assert_eq!((info.removed, info.added, info.is_ancestor), (0, 2, false));
        reset_to(&repo, &c.to_string(), "hard").unwrap();
        assert_eq!(repo.head().unwrap().target(), Some(c));
        assert!(status(&repo).is_empty());

        // Mixed: the staging area follows, the files stay, so the changes are unstaged / untracked.
        reset_to(&repo, &a.to_string(), "mixed").unwrap();
        let st = status(&repo);
        assert!(has(&st, "a.txt", Status::WT_MODIFIED), "{st:?}");
        assert!(has(&st, "c.txt", Status::WT_NEW), "{st:?}");
        assert!(st.iter().all(|(_, s)| !s.intersects(Status::INDEX_NEW | Status::INDEX_MODIFIED)), "{st:?}");
        assert_eq!(fs::read_to_string(dir.join("c.txt")).unwrap(), "sea");

        // Hard: files and index go back too. Uncommitted edits and the removed commits' files are gone;
        // untracked files are left alone.
        reset_to(&repo, &c.to_string(), "hard").unwrap();
        fs::write(dir.join("a.txt"), "uncommitted").unwrap();
        fs::write(dir.join("untracked.txt"), "keep me").unwrap();
        assert_eq!(reset_info(&repo, &a.to_string()).unwrap().working_changes, 2, "the edit and the untracked file");
        reset_to(&repo, &a.to_string(), "hard").unwrap();
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "1");
        assert!(!dir.join("b.txt").exists() && !dir.join("c.txt").exists());
        assert_eq!(fs::read_to_string(dir.join("untracked.txt")).unwrap(), "keep me");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn resetting_to_the_current_commit_and_other_special_cases() {
        let (dir, repo, a, b, c) = history("special");

        // At HEAD itself: mixed unstages, hard discards. The info says it is the same commit.
        fs::write(dir.join("a.txt"), "edited").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let info = reset_info(&repo, &c.to_string()).unwrap();
        assert!(info.same_commit && info.is_ancestor);
        assert_eq!((info.removed, info.added), (0, 0));
        reset_to(&repo, &c.to_string(), "mixed").unwrap();
        assert!(has(&status(&repo), "a.txt", Status::WT_MODIFIED));
        reset_to(&repo, &c.to_string(), "hard").unwrap();
        assert!(status(&repo).is_empty());
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "2");

        // Pushed commits are counted: upstream at b; resetting back to a removes c (local) and b (pushed).
        let branch = repo.head().unwrap().shorthand().unwrap().to_string();
        repo.remote("origin", "https://example.com/r.git").unwrap();
        repo.reference(&format!("refs/remotes/origin/{branch}"), b, true, "test").unwrap();
        repo.find_branch(&branch, BranchType::Local).unwrap().set_upstream(Some(&format!("origin/{branch}"))).unwrap();
        let info = reset_info(&repo, &a.to_string()).unwrap();
        assert_eq!((info.removed, info.pushed_removed), (2, 1));

        // A commit on another line of history is allowed: the branch jumps there.
        let sig = Signature::now("R", "r@example.com").unwrap();
        let tree = repo.find_commit(a).unwrap().tree().unwrap();
        let side = repo.commit(None, &sig, &sig, "side", &tree, &[&repo.find_commit(a).unwrap()]).unwrap();
        let info = reset_info(&repo, &side.to_string()).unwrap();
        assert!(!info.is_ancestor && info.removed == 2 && info.added == 1);

        // A detached HEAD can be reset too, and bad input is refused.
        repo.set_head_detached(c).unwrap();
        assert_eq!(reset_info(&repo, &a.to_string()).unwrap().branch, None);
        reset_to(&repo, &a.to_string(), "hard").unwrap();
        assert_eq!(repo.head().unwrap().target(), Some(a));
        assert!(!repo.head().unwrap().is_branch());
        assert!(reset_to(&repo, &a.to_string(), "sideways").unwrap_err().contains("Unknown reset mode"));
        assert!(reset_to(&repo, "not-an-id", "soft").is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stashes_and_empty_repositories_are_refused() {
        let (dir, mut repo, _a, _b, c) = history("refused");
        fs::write(dir.join("a.txt"), "stash me").unwrap();
        let stash = crate::sidebar::stash::save_stash(&mut repo, Some("wip")).unwrap();
        assert!(reset_info(&repo, &stash.to_string()).unwrap_err().contains("stash"));
        assert!(reset_to(&repo, &stash.to_string(), "hard").unwrap_err().contains("stash"));
        assert_eq!(repo.head().unwrap().target(), Some(c), "nothing moved");
        let _ = fs::remove_dir_all(&dir);

        let empty = std::env::temp_dir().join(format!("gc-reset-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&empty);
        let repo = Repository::init(&empty).unwrap();
        assert!(reset_info(&repo, "0123456789012345678901234567890123456789").unwrap_err().contains("no commits"));
        let _ = fs::remove_dir_all(&empty);
    }
}
