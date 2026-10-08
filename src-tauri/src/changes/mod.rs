pub mod hunks;

use std::path::Path;

use git2::build::CheckoutBuilder;
use git2::{ObjectType, Oid, Repository, RepositoryState, Status, StatusOptions};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    /// Status in the index (staged), if any: "new" | "modified" | "deleted" | "typechange".
    pub staged: Option<&'static str>,
    /// Status in the working tree (unstaged), if any; also "conflicted".
    pub unstaged: Option<&'static str>,
}

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

pub(crate) fn status_of(repo: &Repository) -> Result<Vec<FileChange>, String> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        // Renames are reported as delete + add so each path can be (un)staged independently.
        .renames_head_to_index(false)
        .renames_index_to_workdir(false);

    let statuses = repo.statuses(Some(&mut opts)).map_err(err)?;
    let mut out = Vec::new();
    for entry in statuses.iter() {
        let Ok(path) = entry.path() else { continue };
        let s = entry.status();

        let staged = if s.contains(Status::INDEX_NEW) {
            Some("new")
        } else if s.contains(Status::INDEX_MODIFIED) {
            Some("modified")
        } else if s.contains(Status::INDEX_DELETED) {
            Some("deleted")
        } else if s.contains(Status::INDEX_TYPECHANGE) {
            Some("typechange")
        } else {
            None
        };
        let unstaged = if s.contains(Status::CONFLICTED) {
            Some("conflicted")
        } else if s.contains(Status::WT_NEW) {
            Some("new")
        } else if s.contains(Status::WT_MODIFIED) {
            Some("modified")
        } else if s.contains(Status::WT_DELETED) {
            Some("deleted")
        } else if s.contains(Status::WT_TYPECHANGE) {
            Some("typechange")
        } else {
            None
        };

        if staged.is_some() || unstaged.is_some() {
            out.push(FileChange { path: path.to_string(), staged, unstaged });
        }
    }
    Ok(out)
}

fn stage(repo: &Repository, paths: &[String]) -> Result<(), String> {
    let workdir = repo.workdir().ok_or("repository has no working directory")?;
    let mut index = repo.index().map_err(err)?;
    for p in paths {
        let rel = Path::new(p);
        if workdir.join(rel).symlink_metadata().is_ok() {
            index.add_path(rel).map_err(err)?;
        } else {
            index.remove_path(rel).map_err(err)?; // deleted in the working tree
        }
    }
    index.write().map_err(err)
}

fn unstage(repo: &Repository, paths: &[String]) -> Result<(), String> {
    match repo.head() {
        Ok(head) => {
            let target = head.peel(ObjectType::Commit).map_err(err)?;
            repo.reset_default(Some(&target), paths.iter().map(String::as_str)).map_err(err)
        }
        // No commits yet: unstaging just means dropping the entry from the index.
        Err(_) => {
            let mut index = repo.index().map_err(err)?;
            for p in paths {
                index.remove_path(Path::new(p)).map_err(err)?;
            }
            index.write().map_err(err)
        }
    }
}

/// Throws away the unstaged changes of whole files: a modified or deleted file goes back to what the
/// index holds (so anything staged is kept), an untracked file is deleted. Only paths that really have
/// unstaged changes are accepted, and conflicted files are refused (there is no conflict UI yet).
fn discard(repo: &Repository, paths: &[String]) -> Result<(), String> {
    let status = status_of(repo)?;
    let mut restore = Vec::new();
    let mut delete = Vec::new();
    for p in paths {
        let kind = status
            .iter()
            .find(|c| c.path == *p)
            .and_then(|c| c.unstaged)
            .ok_or_else(|| format!("{p} has no unstaged changes to discard"))?;
        match kind {
            "conflicted" => return Err(format!("{p} has merge conflicts; resolve them first")),
            "new" => delete.push(p),
            _ => restore.push(p),
        }
    }

    if !restore.is_empty() {
        let mut checkout = CheckoutBuilder::new();
        checkout.force().disable_pathspec_match(true);
        for p in &restore {
            checkout.path(p);
        }
        let mut index = repo.index().map_err(err)?;
        repo.checkout_index(Some(&mut index), Some(&mut checkout)).map_err(err)?;
    }
    let workdir = repo.workdir().ok_or("repository has no working directory")?;
    for p in delete {
        std::fs::remove_file(workdir.join(p)).map_err(|e| format!("Could not delete {p}: {e}"))?;
    }
    Ok(())
}

fn commit_staged(repo: &mut Repository, message: &str) -> Result<Oid, String> {
    let message = git2::message_prettify(message, None).map_err(err)?;
    if message.trim().is_empty() {
        return Err("Commit message is empty".into());
    }
    let sig = repo.signature().map_err(|_| "Git identity not set. Configure user.name and user.email.".to_string())?;

    // Concluding a merge: the merge heads become additional parents.
    // (Collected first because mergehead_foreach needs `&mut repo`.)
    let merging = repo.state() == RepositoryState::Merge;
    let mut merge_heads = Vec::new();
    if merging {
        repo.mergehead_foreach(|oid| {
            merge_heads.push(*oid);
            true
        })
        .map_err(err)?;
    }

    let mut index = repo.index().map_err(err)?;
    if index.has_conflicts() {
        return Err("Resolve conflicts before committing".into());
    }
    let tree = repo.find_tree(index.write_tree().map_err(err)?).map_err(err)?;

    let mut parents = Vec::new();
    if let Ok(head) = repo.head() {
        parents.push(head.peel_to_commit().map_err(err)?);
    }
    for oid in merge_heads {
        parents.push(repo.find_commit(oid).map_err(err)?);
    }

    if !merging && parents.first().is_some_and(|p| p.tree_id() == tree.id()) {
        return Err("Nothing staged to commit".into());
    }
    if !merging && parents.is_empty() && tree.is_empty() {
        return Err("Nothing staged to commit".into());
    }

    let parent_refs: Vec<_> = parents.iter().collect();
    let oid = repo.commit(Some("HEAD"), &sig, &sig, &message, &tree, &parent_refs).map_err(err)?;
    if merging {
        repo.cleanup_state().map_err(err)?;
    }
    Ok(oid)
}

/// Replaces the last commit with the current index and `message`, like `git commit --amend`.
/// The author (and author date) is kept; the committer becomes the current identity.
fn amend_head(repo: &Repository, message: &str) -> Result<Oid, String> {
    let message = git2::message_prettify(message, None).map_err(err)?;
    if message.trim().is_empty() {
        return Err("Commit message is empty".into());
    }
    if repo.state() == RepositoryState::Merge {
        return Err("Finish the merge before amending".into());
    }
    let head =
        repo.head().and_then(|h| h.peel_to_commit()).map_err(|_| "There is no previous commit to amend".to_string())?;
    let committer =
        repo.signature().map_err(|_| "Git identity not set. Configure user.name and user.email.".to_string())?;

    let mut index = repo.index().map_err(err)?;
    if index.has_conflicts() {
        return Err("Resolve conflicts before committing".into());
    }
    let tree = repo.find_tree(index.write_tree().map_err(err)?).map_err(err)?;
    if tree.id() == head.tree_id() && head.message().unwrap_or("").trim() == message.trim() {
        return Err("Nothing to amend: no staged changes and the message is unchanged".into());
    }

    head.amend(Some("HEAD"), None, Some(&committer), None, Some(&message), Some(&tree)).map_err(err)
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HeadCommit {
    pub short_id: String,
    pub message: String,
    /// The commit is already on the branch's upstream, so amending it needs a force push.
    pub pushed: bool,
}

/// The last commit, or None on a branch without commits.
fn head_commit(repo: &Repository) -> Option<HeadCommit> {
    let commit = repo.head().ok()?.peel_to_commit().ok()?;

    let pushed = crate::history::is_pushed(repo, commit.id());
    Some(HeadCommit {
        short_id: commit.id().to_string()[..7].to_string(),
        message: commit.message().unwrap_or("").trim_end().to_string(),
        pushed,
    })
}

async fn blocking<T, F>(path: String, f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&mut Repository) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let mut repo = Repository::discover(&path).map_err(err)?;
        f(&mut repo)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_status(path: String) -> Result<Vec<FileChange>, String> {
    blocking(path, |r| status_of(r)).await
}

#[tauri::command]
pub async fn stage_paths(path: String, paths: Vec<String>) -> Result<(), String> {
    let label = format!("Stage {}", crate::undo::describe_paths(&paths));
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Index, || blocking(path, move |r| stage(r, &paths)))
        .await
}

#[tauri::command]
pub async fn unstage_paths(path: String, paths: Vec<String>) -> Result<(), String> {
    let label = format!("Unstage {}", crate::undo::describe_paths(&paths));
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Index, || {
        blocking(path, move |r| unstage(r, &paths))
    })
    .await
}

/// Discards the unstaged changes of whole files (untracked files are deleted). Not undoable.
#[tauri::command]
pub async fn discard_paths(path: String, paths: Vec<String>) -> Result<(), String> {
    let label = format!("Discard changes to {}", crate::undo::describe_paths(&paths));
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Full, || blocking(path, move |r| discard(r, &paths)))
        .await
}

/// Commits the index, or with `amend` replaces the last commit. Returns the commit id.
#[tauri::command]
pub async fn create_commit(path: String, message: String, amend: bool) -> Result<String, String> {
    let label = if amend { "Amend commit" } else { "Commit" };
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Keep, || {
        blocking(path, move |r| {
            if amend { amend_head(r, &message) } else { commit_staged(r, &message) }.map(|o| o.to_string())
        })
    })
    .await
}

/// Details of the last commit (to pre-fill the amend message and warn about history rewrites).
#[tauri::command]
pub async fn get_head_commit(path: String) -> Result<Option<HeadCommit>, String> {
    blocking(path, |r| Ok(head_commit(r))).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::BranchType;
    use std::fs;

    fn find<'a>(list: &'a [FileChange], p: &str) -> &'a FileChange {
        list.iter().find(|c| c.path == p).unwrap()
    }

    #[test]
    fn stage_commit_unstage_flow() {
        let dir = std::env::temp_dir().join(format!("gc-changes-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut repo = Repository::init(&dir).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test").unwrap();
        cfg.set_str("user.email", "test@example.com").unwrap();
        cfg.set_str("core.autocrlf", "false").unwrap(); // the result must not depend on the machine's git config

        fs::write(dir.join("a.txt"), "one").unwrap();
        fs::create_dir(dir.join("sub")).unwrap();
        fs::write(dir.join("sub").join("b.txt"), "two").unwrap();

        let st = status_of(&repo).unwrap();
        assert_eq!(find(&st, "a.txt").unstaged, Some("new"));
        assert_eq!(find(&st, "sub/b.txt").unstaged, Some("new"));

        // Nothing staged yet, including on an unborn branch.
        assert!(commit_staged(&mut repo, "x").is_err());

        stage(&repo, &["a.txt".into(), "sub/b.txt".into()]).unwrap();
        assert_eq!(find(&status_of(&repo).unwrap(), "a.txt").staged, Some("new"));

        // Unstage on an unborn branch.
        unstage(&repo, &["sub/b.txt".into()]).unwrap();
        assert_eq!(find(&status_of(&repo).unwrap(), "sub/b.txt").staged, None);

        assert!(commit_staged(&mut repo, "   ").is_err());
        commit_staged(&mut repo, "first").unwrap();
        let st = status_of(&repo).unwrap();
        assert!(st.iter().all(|c| c.path != "a.txt"));
        assert_eq!(find(&st, "sub/b.txt").unstaged, Some("new"));
        assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().summary(), Ok(Some("first")));

        // Modify + delete, stage both, then unstage one with HEAD present.
        fs::write(dir.join("a.txt"), "changed").unwrap();
        fs::write(dir.join("c.txt"), "three").unwrap();
        stage(&repo, &["a.txt".into(), "c.txt".into()]).unwrap();
        fs::remove_file(dir.join("c.txt")).unwrap();
        stage(&repo, &["c.txt".into()]).unwrap(); // deletion of a never-committed file clears it
        unstage(&repo, &["a.txt".into()]).unwrap();
        let st = status_of(&repo).unwrap();
        assert_eq!(find(&st, "a.txt").staged, None);
        assert_eq!(find(&st, "a.txt").unstaged, Some("modified"));

        // Nothing staged -> refuse.
        assert_eq!(commit_staged(&mut repo, "again").unwrap_err(), "Nothing staged to commit");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn discard_whole_files() {
        let dir = std::env::temp_dir().join(format!("gc-discard-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut repo = Repository::init(&dir).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test").unwrap();
        cfg.set_str("user.email", "test@example.com").unwrap();
        cfg.set_str("core.autocrlf", "false").unwrap();
        for (name, text) in [("mod.txt", "m1"), ("gone.txt", "g1"), ("both.txt", "b1")] {
            fs::write(dir.join(name), text).unwrap();
        }
        stage(&repo, &["mod.txt".into(), "gone.txt".into(), "both.txt".into()]).unwrap();
        commit_staged(&mut repo, "base").unwrap();

        fs::write(dir.join("mod.txt"), "m2").unwrap();
        fs::remove_file(dir.join("gone.txt")).unwrap();
        fs::write(dir.join("both.txt"), "b2").unwrap();
        stage(&repo, &["both.txt".into()]).unwrap(); // staged b2 ...
        fs::write(dir.join("both.txt"), "b3").unwrap(); // ... plus a later edit
        fs::create_dir(dir.join("sub")).unwrap();
        fs::write(dir.join("sub").join("new.txt"), "n").unwrap();
        fs::write(dir.join("keep.txt"), "k").unwrap();

        // Paths without unstaged changes are refused, and nothing happens at all in that case.
        let e = discard(&repo, &["mod.txt".into(), "nope.txt".into()]).unwrap_err();
        assert!(e.contains("nope.txt"), "{e}");
        assert_eq!(fs::read_to_string(dir.join("mod.txt")).unwrap(), "m2");

        discard(&repo, &["mod.txt".into(), "gone.txt".into(), "both.txt".into(), "sub/new.txt".into()]).unwrap();
        assert_eq!(fs::read_to_string(dir.join("mod.txt")).unwrap(), "m1");
        assert_eq!(fs::read_to_string(dir.join("gone.txt")).unwrap(), "g1", "a deleted file comes back");
        assert_eq!(fs::read_to_string(dir.join("both.txt")).unwrap(), "b2", "back to the staged version");
        assert!(!dir.join("sub").join("new.txt").exists(), "an untracked file is deleted");
        assert!(dir.join("keep.txt").exists(), "other files are untouched");

        let st = status_of(&repo).unwrap();
        assert_eq!(find(&st, "both.txt").staged, Some("modified"), "the staged change stays");
        assert_eq!(find(&st, "both.txt").unstaged, None);
        assert_eq!(
            st.len(),
            2,
            "only both.txt and keep.txt are left: {:?}",
            st.iter().map(|c| &c.path).collect::<Vec<_>>()
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn amend_rewrites_last_commit() {
        let dir = std::env::temp_dir().join(format!("gc-amend-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut repo = Repository::init(&dir).unwrap();
        let set_user = |repo: &Repository, name: &str| {
            let mut cfg = repo.config().unwrap();
            cfg.set_str("user.name", name).unwrap();
            cfg.set_str("user.email", "t@example.com").unwrap();
            cfg.set_str("core.autocrlf", "false").unwrap(); // the result must not depend on the machine's git config
        };
        set_user(&repo, "Original");

        // Nothing to amend yet.
        assert!(head_commit(&repo).is_none());
        assert!(amend_head(&repo, "x").unwrap_err().contains("no previous commit"));

        fs::write(dir.join("a.txt"), "one").unwrap();
        stage(&repo, &["a.txt".into()]).unwrap();
        let first = commit_staged(&mut repo, "first").unwrap();
        let info = head_commit(&repo).unwrap();
        assert_eq!((info.message.as_str(), info.pushed), ("first", false));

        // Nothing changed -> refuse; empty message -> refuse.
        assert!(amend_head(&repo, "first").unwrap_err().contains("Nothing to amend"));
        assert!(amend_head(&repo, "  ").is_err());

        // Message-only amend by someone else: author stays, committer changes, history unchanged.
        set_user(&repo, "Other");
        let second = amend_head(&repo, "first, reworded").unwrap();
        assert_ne!(first, second);
        let c = repo.find_commit(second).unwrap();
        assert_eq!(c.message(), Ok("first, reworded\n"));
        assert_eq!(c.author().name(), Ok("Original"));
        assert_eq!(c.committer().name(), Ok("Other"));
        assert_eq!(c.parent_count(), 0);
        assert_eq!(repo.head().unwrap().target(), Some(second));

        // Staged changes are folded into the amended commit.
        fs::write(dir.join("a.txt"), "two").unwrap();
        fs::write(dir.join("b.txt"), "new").unwrap();
        stage(&repo, &["a.txt".into(), "b.txt".into()]).unwrap();
        let third = amend_head(&repo, "first, with more").unwrap();
        let tree = repo.find_commit(third).unwrap().tree().unwrap();
        assert_eq!(tree.len(), 2);
        assert!(status_of(&repo).unwrap().is_empty());

        // Once the commit is on the upstream, amending it is flagged as a history rewrite.
        repo.remote("origin", "https://example.com/r.git").unwrap();
        let branch = repo.head().unwrap().shorthand().unwrap().to_string();
        repo.reference(&format!("refs/remotes/origin/{branch}"), third, true, "test").unwrap();
        repo.find_branch(&branch, BranchType::Local).unwrap().set_upstream(Some(&format!("origin/{branch}"))).unwrap();
        assert!(head_commit(&repo).unwrap().pushed);
        fs::write(dir.join("c.txt"), "x").unwrap();
        stage(&repo, &["c.txt".into()]).unwrap();
        amend_head(&repo, "first, with more").unwrap();
        assert!(!head_commit(&repo).unwrap().pushed);

        let _ = fs::remove_dir_all(&dir);
    }
}
