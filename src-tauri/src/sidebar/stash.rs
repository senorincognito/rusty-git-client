use git2::build::CheckoutBuilder;
use git2::{Commit, ErrorCode, ObjectType, Oid, Repository, ResetType, StashApplyOptions, StashFlags, Tree};
use serde::Serialize;

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    /// Position in the stash list: 0 is the newest ("stash@{0}").
    pub index: usize,
    pub id: String,
    pub short_id: String,
    /// "WIP on main: abc1234 subject", or "On main: <your message>".
    pub message: String,
    /// Unix seconds.
    pub time: i64,
}

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

/// All stashes, newest first.
pub(crate) fn list_stashes(repo: &mut Repository) -> Result<Vec<StashEntry>, String> {
    let mut raw: Vec<(usize, String, Oid)> = Vec::new();
    repo.stash_foreach(|index, message, oid| {
        raw.push((index, message.to_string(), *oid));
        true
    })
    .map_err(err)?;

    Ok(raw
        .into_iter()
        .map(|(index, message, oid)| StashEntry {
            index,
            id: oid.to_string(),
            short_id: oid.to_string()[..7].to_string(),
            message,
            time: repo.find_commit(oid).map(|c| c.time().seconds()).unwrap_or(0),
        })
        .collect())
}

/// The position of `oid` in the stash list, if it is a stash commit. Works on a shared
/// reference by looking at the list through a second handle.
pub(crate) fn stash_index_of(repo: &Repository, oid: Oid) -> Option<usize> {
    let mut other = Repository::open(repo.path()).ok()?;
    list_stashes(&mut other).ok()?.iter().find(|s| s.id == oid.to_string()).map(|s| s.index)
}

/// The tree of a stash's untracked files, which git keeps in a separate third parent commit.
pub(crate) fn untracked_tree<'r>(repo: &'r Repository, commit: &Commit<'r>) -> Option<Tree<'r>> {
    if commit.parent_count() == 3 && stash_index_of(repo, commit.id()).is_some() {
        commit.parent(2).ok()?.tree().ok()
    } else {
        None
    }
}

/// Moves every uncommitted change into a new stash and cleans the working directory: staged and
/// unstaged edits, and untracked files (ignored files stay where they are).
pub(crate) fn save_stash(repo: &mut Repository, message: Option<&str>) -> Result<Oid, String> {
    let signature =
        repo.signature().map_err(|_| "Git identity not set. Configure user.name and user.email.".to_string())?;
    if repo.head().and_then(|h| h.peel_to_commit()).is_err() {
        return Err("Make a first commit before stashing".into());
    }
    let message = message.map(str::trim).filter(|m| !m.is_empty());
    match repo.stash_save2(&signature, message, Some(StashFlags::INCLUDE_UNTRACKED)) {
        Ok(oid) => Ok(oid),
        Err(e) if e.code() == ErrorCode::NotFound => Err("There are no changes to stash".into()),
        Err(e) => Err(err(e)),
    }
}

/// Like [`save_stash`], but only for the given files (`git stash push --include-untracked -- <paths>`):
/// their staged and unstaged edits and, for untracked ones, the files themselves. Everything else stays
/// in the working directory. It runs the system git: libgit2's own path-limited stash also cleans the
/// files that were not selected. Paths are literal (`--literal-pathspecs`), so names like "[x].txt" are fine.
pub(crate) fn save_stash_paths(repo: &Repository, paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("No files to stash".into());
    }
    if repo.head().and_then(|h| h.peel_to_commit()).is_err() {
        return Err("Make a first commit before stashing".into());
    }
    let known = crate::changes::status_of(repo)?;
    if let Some(p) = paths.iter().find(|p| !known.iter().any(|c| &c.path == *p)) {
        return Err(format!("{p} has no uncommitted changes to stash"));
    }
    let workdir = repo.workdir().and_then(|w| w.to_str()).ok_or("repository has no working directory")?;

    let mut args = vec!["--literal-pathspecs", "stash", "push", "--include-untracked", "--"];
    args.extend(paths.iter().map(String::as_str));
    crate::toolbar::sync::run_git(workdir, &args).map(|_| ())
}

/// Puts the working directory back to a clean checkout of HEAD, untracked files included.
fn restore_clean(repo: &Repository) {
    if let Ok(head) = repo.head().and_then(|h| h.peel(ObjectType::Commit)) {
        let _ = repo.reset(&head, ResetType::Hard, None);
    }
    let mut checkout = CheckoutBuilder::new();
    checkout.force().remove_untracked(true);
    let _ = repo.checkout_head(Some(&mut checkout));
}

/// Applies a stash to the working directory and removes it from the list (`git stash pop`),
/// putting back what was staged as staged. The stash is found by its commit id, because list
/// positions shift when other stashes come and go.
///
/// It needs a clean working directory. There is no conflict resolution in the app yet, so this
/// guarantees a conflicting pop can be undone completely: the repository is restored to the clean
/// state it was in and the stash is kept.
/// Applies the stash and removes it from the list (git stash pop).
fn pop_stash(repo: &mut Repository, id: &str) -> Result<(), String> {
    apply_stash(repo, id, true)
}

/// Applies the stash to a clean working directory, re-staging what was staged; with `remove` it is dropped afterwards.
/// A conflicting apply is undone and the stash is kept either way.
fn apply_stash(repo: &mut Repository, id: &str, remove: bool) -> Result<(), String> {
    let oid = Oid::from_str(id).map_err(err)?;
    let index = stash_index_of(repo, oid).ok_or("That stash no longer exists")?;
    if repo.head().and_then(|h| h.peel_to_commit()).is_err() {
        return Err("Make a first commit before applying a stash".into());
    }
    if !crate::changes::status_of(repo)?.is_empty() {
        return Err("The working directory has uncommitted changes. Commit or stash them first, so the stash \
                    can be applied cleanly and undone if it conflicts."
            .into());
    }

    // Apply first and drop only once it is known to be clean: libgit2's own pop reports success for a
    // conflicting apply (leaving conflict markers behind) and still removes the stash, unlike git.
    let mut options = StashApplyOptions::new();
    options.reinstantiate_index();
    let applied = repo.stash_apply(index, Some(&mut options));
    let conflicted = repo
        .index()
        .map(|mut i| {
            let _ = i.read(true);
            i.has_conflicts()
        })
        .unwrap_or(false);
    match applied {
        Ok(()) if !conflicted => {
            if remove {
                repo.stash_drop(index).map_err(err)?;
            }
            Ok(())
        }
        outcome => {
            restore_clean(repo);
            let reason = match outcome {
                Err(e) => e.message().trim().to_string(),
                Ok(()) => "it conflicts with the files in the working directory".to_string(),
            };
            Err(format!(
                "The stash could not be applied cleanly ({reason}). Nothing was changed and the stash was kept."
            ))
        }
    }
}

/// Removes a stash without applying it. The stash commit stays in the object database (and the
/// reflog of refs/stash) for a while, but its changes are gone from the list for good.
fn drop_stash(repo: &mut Repository, id: &str) -> Result<(), String> {
    let oid = Oid::from_str(id).map_err(err)?;
    let index = stash_index_of(repo, oid).ok_or("That stash no longer exists")?;
    repo.stash_drop(index).map_err(err)
}

#[tauri::command]
pub async fn get_stashes(path: String) -> Result<Vec<StashEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || list_stashes(&mut Repository::discover(&path).map_err(err)?))
        .await
        .map_err(|e| e.to_string())?
}

/// Stashes all uncommitted changes, with an optional message. Returns the stash commit id.
#[tauri::command]
pub async fn create_stash(path: String, message: Option<String>) -> Result<String, String> {
    crate::undo::recorded(&path.clone(), "Stash changes", crate::undo::Kind::Full, || async move {
        tauri::async_runtime::spawn_blocking(move || {
            let mut repo = Repository::discover(&path).map_err(err)?;
            save_stash(&mut repo, message.as_deref()).map(|o| o.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    })
    .await
}

/// Stashes only the given files.
#[tauri::command]
pub async fn stash_paths_cmd(path: String, paths: Vec<String>) -> Result<(), String> {
    let label = format!("Stash {}", crate::undo::describe_paths(&paths));
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Full, || async move {
        tauri::async_runtime::spawn_blocking(move || {
            save_stash_paths(&Repository::discover(&path).map_err(err)?, &paths)
        })
        .await
        .map_err(|e| e.to_string())?
    })
    .await
}

/// Applies a stash and removes it from the list. Needs a clean working directory.
#[tauri::command]
pub async fn pop_stash_cmd(path: String, id: String) -> Result<(), String> {
    crate::undo::recorded(&path.clone(), "Pop stash", crate::undo::Kind::Full, || async move {
        tauri::async_runtime::spawn_blocking(move || {
            let mut repo = Repository::discover(&path).map_err(err)?;
            pop_stash(&mut repo, &id)
        })
        .await
        .map_err(|e| e.to_string())?
    })
    .await
}

/// Applies a stash and keeps it in the list. Needs a clean working directory.
#[tauri::command]
pub async fn apply_stash_cmd(path: String, id: String) -> Result<(), String> {
    crate::undo::recorded(&path.clone(), "Apply stash", crate::undo::Kind::Full, || async move {
        tauri::async_runtime::spawn_blocking(move || {
            let mut repo = Repository::discover(&path).map_err(err)?;
            apply_stash(&mut repo, &id, false)
        })
        .await
        .map_err(|e| e.to_string())?
    })
    .await
}

/// Deletes every stash without applying any. Returns how many there were.
fn drop_all_stashes(repo: &mut Repository) -> Result<usize, String> {
    let count = list_stashes(repo)?.len();
    // Always the newest one: the others move up, so the position stays 0.
    for _ in 0..count {
        repo.stash_drop(0).map_err(err)?;
    }
    Ok(count)
}

/// Deletes every stash without applying any. One undo step brings them all back.
#[tauri::command]
pub async fn drop_all_stashes_cmd(path: String) -> Result<usize, String> {
    crate::undo::recorded(&path.clone(), "Delete all stashes", crate::undo::Kind::Keep, || async move {
        tauri::async_runtime::spawn_blocking(move || {
            let mut repo = Repository::discover(&path).map_err(err)?;
            drop_all_stashes(&mut repo)
        })
        .await
        .map_err(|e| e.to_string())?
    })
    .await
}

/// Deletes a stash without applying it.
#[tauri::command]
pub async fn drop_stash_cmd(path: String, id: String) -> Result<(), String> {
    crate::undo::recorded(&path.clone(), "Delete stash", crate::undo::Kind::Keep, || async move {
        tauri::async_runtime::spawn_blocking(move || {
            let mut repo = Repository::discover(&path).map_err(err)?;
            drop_stash(&mut repo, &id)
        })
        .await
        .map_err(|e| e.to_string())?
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{IndexAddOption, Signature, StatusOptions};
    use std::fs;

    fn setup(name: &str) -> (std::path::PathBuf, Repository) {
        let dir = std::env::temp_dir().join(format!("gc-stash-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Stasher").unwrap();
        cfg.set_str("user.email", "s@example.com").unwrap();
        cfg.set_str("core.autocrlf", "false").unwrap(); // the result must not depend on the machine's git config
        (dir, repo)
    }

    fn commit_all(repo: &Repository, msg: &str) {
        let mut index = repo.index().unwrap();
        index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("Stasher", "s@example.com").unwrap();
        let parents: Vec<_> = repo.head().ok().and_then(|h| h.peel_to_commit().ok()).into_iter().collect();
        let refs: Vec<_> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &refs).unwrap();
    }

    fn dirty_files(repo: &Repository) -> usize {
        let mut o = StatusOptions::new();
        o.include_untracked(true).recurse_untracked_dirs(true);
        repo.statuses(Some(&mut o)).unwrap().len()
    }

    #[test]
    fn stashing_moves_all_changes_away_and_lists_them_newest_first() {
        let (dir, mut repo) = setup("save");

        // Nothing to stash yet, and no commit to base a stash on.
        assert!(save_stash(&mut repo, None).unwrap_err().contains("first commit"));
        fs::write(dir.join("a.txt"), "one").unwrap();
        fs::write(dir.join("keep.txt"), "keep").unwrap();
        commit_all(&repo, "base");
        assert!(save_stash(&mut repo, None).unwrap_err().contains("no changes"));
        assert!(list_stashes(&mut repo).unwrap().is_empty());

        // A tracked edit, a newly staged file and an untracked file all go into the stash.
        fs::write(dir.join("a.txt"), "two").unwrap();
        fs::write(dir.join("staged.txt"), "staged").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("staged.txt")).unwrap();
        index.write().unwrap();
        fs::write(dir.join("untracked.txt"), "untracked").unwrap();
        assert_eq!(dirty_files(&repo), 3);

        let first = save_stash(&mut repo, Some("  my first stash  ")).unwrap();
        assert_eq!(dirty_files(&repo), 0, "the working directory is clean afterwards");
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "one");
        assert!(!dir.join("untracked.txt").exists() && !dir.join("staged.txt").exists());

        // A second stash (without a message) goes on top.
        fs::write(dir.join("a.txt"), "three").unwrap();
        let second = save_stash(&mut repo, None).unwrap();
        let list = list_stashes(&mut repo).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!((list[0].index, list[0].id.as_str()), (0, second.to_string().as_str()));
        assert_eq!((list[1].index, list[1].id.as_str()), (1, first.to_string().as_str()));
        assert!(list[1].message.contains("my first stash"), "{}", list[1].message);
        assert!(list[0].message.starts_with("WIP on"), "{}", list[0].message);
        assert_eq!(list[0].short_id.len(), 7);

        // Helpers used by the graph and the detail view.
        assert_eq!(stash_index_of(&repo, second), Some(0));
        assert_eq!(stash_index_of(&repo, first), Some(1));
        assert_eq!(stash_index_of(&repo, repo.head().unwrap().target().unwrap()), None);
        let stash = repo.find_commit(first).unwrap();
        assert!(untracked_tree(&repo, &stash).unwrap().get_name("untracked.txt").is_some());
        let plain = repo.find_commit(second).unwrap();
        // libgit2 records an untracked-files commit even when there are none: it is just empty.
        assert!(untracked_tree(&repo, &plain).is_none_or(|t| t.is_empty()), "no untracked files in the second stash");

        let _ = fs::remove_dir_all(&dir);
    }

    fn status_map(repo: &Repository) -> Vec<(String, git2::Status)> {
        let mut o = StatusOptions::new();
        o.include_untracked(true).recurse_untracked_dirs(true);
        repo.statuses(Some(&mut o)).unwrap().iter().map(|e| (e.path().unwrap().to_string(), e.status())).collect()
    }

    #[test]
    fn stashing_selected_files_leaves_the_rest() {
        let (dir, mut repo) = setup("paths");
        for name in ["a.txt", "b.txt", "[x].txt", "x.txt"] {
            fs::write(dir.join(name), "base").unwrap();
        }
        commit_all(&repo, "base");

        fs::write(dir.join("a.txt"), "edited").unwrap();
        fs::write(dir.join("b.txt"), "edited").unwrap();
        fs::write(dir.join("[x].txt"), "edited").unwrap();
        fs::write(dir.join("x.txt"), "edited").unwrap();
        fs::write(dir.join("new.txt"), "untracked").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("a.txt")).unwrap(); // a.txt is staged

        index.write().unwrap();

        assert!(save_stash_paths(&repo, &[]).is_err());
        assert!(save_stash_paths(&repo, &["nope.txt".into()]).unwrap_err().contains("nope.txt"));

        // A name with glob characters matches only itself ("[x].txt" must not also take "x.txt").
        save_stash_paths(&repo, &["[x].txt".into()]).unwrap();
        assert_eq!(fs::read_to_string(dir.join("[x].txt")).unwrap(), "base");
        assert_eq!(fs::read_to_string(dir.join("x.txt")).unwrap(), "edited");

        // A staged file and an untracked file go; the other edited file stays.
        save_stash_paths(&repo, &["a.txt".into(), "new.txt".into()]).unwrap();
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "base");
        assert!(!dir.join("new.txt").exists());
        assert_eq!(fs::read_to_string(dir.join("b.txt")).unwrap(), "edited");

        let list = list_stashes(&mut repo).unwrap();
        assert_eq!(list.len(), 2);

        // The stash holds what was set aside: popping it brings back the staged edit and the new file.
        pop_stash_after_cleaning(&mut repo, &list[0].id);
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "edited");
        assert_eq!(fs::read_to_string(dir.join("new.txt")).unwrap(), "untracked");

        let _ = fs::remove_dir_all(&dir);
    }

    // pop_stash needs a clean working directory: put the remaining edits aside first.
    fn pop_stash_after_cleaning(repo: &mut Repository, id: &str) {
        save_stash(repo, Some("rest")).unwrap();
        pop_stash(repo, id).unwrap();
    }

    #[test]
    fn drop_removes_only_the_chosen_stash() {
        let (dir, mut repo) = setup("drop");
        fs::write(dir.join("a.txt"), "one").unwrap();
        commit_all(&repo, "base");
        fs::write(dir.join("a.txt"), "two").unwrap();
        let older = save_stash(&mut repo, Some("older")).unwrap();
        fs::write(dir.join("b.txt"), "b").unwrap();
        let newer = save_stash(&mut repo, Some("newer")).unwrap();

        // Dropping works with the working directory dirty and does not touch it.
        fs::write(dir.join("a.txt"), "dirty").unwrap();
        drop_stash(&mut repo, &older.to_string()).unwrap();
        let list = list_stashes(&mut repo).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, newer.to_string());
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "dirty");

        assert!(drop_stash(&mut repo, &older.to_string()).unwrap_err().contains("no longer exists"));
        drop_stash(&mut repo, &newer.to_string()).unwrap();
        assert!(list_stashes(&mut repo).unwrap().is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pop_restores_changes_and_drops_the_stash() {
        let (dir, mut repo) = setup("pop");
        fs::write(dir.join("a.txt"), "one").unwrap();
        commit_all(&repo, "base");

        // Stash a tracked edit, a staged new file and an untracked file; pop brings all of them back.
        fs::write(dir.join("a.txt"), "two").unwrap();
        fs::write(dir.join("staged.txt"), "staged").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("staged.txt")).unwrap();
        index.write().unwrap();
        fs::write(dir.join("untracked.txt"), "untracked").unwrap();
        let older = save_stash(&mut repo, Some("older")).unwrap();
        fs::write(dir.join("b.txt"), "b").unwrap();
        let newer = save_stash(&mut repo, Some("newer")).unwrap();

        // The stash is found by id even though its list position is 1, not 0.
        pop_stash(&mut repo, &older.to_string()).unwrap();
        let list = list_stashes(&mut repo).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, newer.to_string(), "the other stash is untouched");
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "two");
        assert_eq!(fs::read_to_string(dir.join("untracked.txt")).unwrap(), "untracked");
        let status = status_map(&repo);
        let flags = |p: &str| status.iter().find(|(n, _)| n == p).map(|(_, s)| *s).unwrap();
        assert!(flags("a.txt").contains(git2::Status::WT_MODIFIED));
        assert!(flags("staged.txt").contains(git2::Status::INDEX_NEW), "staged stays staged");
        assert!(flags("untracked.txt").contains(git2::Status::WT_NEW));

        // A stash that is gone, and a dirty working directory, are refused.
        assert!(pop_stash(&mut repo, &older.to_string()).unwrap_err().contains("no longer exists"));
        let e = pop_stash(&mut repo, &newer.to_string()).unwrap_err();
        assert!(e.contains("uncommitted changes"), "{e}");
        assert_eq!(list_stashes(&mut repo).unwrap().len(), 1, "a refused pop keeps the stash");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_conflicting_pop_is_undone_and_keeps_the_stash() {
        let (dir, mut repo) = setup("pop-conflict");
        fs::write(dir.join("a.txt"), "one").unwrap();
        commit_all(&repo, "base");

        fs::write(dir.join("a.txt"), "stash version").unwrap();
        fs::write(dir.join("from-stash.txt"), "untracked in the stash").unwrap();
        let stash = save_stash(&mut repo, Some("conflicting")).unwrap();

        // Meanwhile the same file was committed differently.
        fs::write(dir.join("a.txt"), "committed version").unwrap();
        commit_all(&repo, "meanwhile");

        let e = pop_stash(&mut repo, &stash.to_string()).unwrap_err();
        assert!(e.contains("could not be applied") && e.contains("stash was kept"), "{e}");
        // Exactly as before: clean tree, our content, no leftovers from the failed pop, stash still there.
        assert!(status_map(&repo).is_empty(), "{:?}", status_map(&repo));
        assert_eq!(repo.state(), git2::RepositoryState::Clean);
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "committed version");
        assert!(!dir.join("from-stash.txt").exists());
        assert_eq!(list_stashes(&mut repo).unwrap().len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_restores_changes_and_keeps_the_stash() {
        let (dir, mut repo) = setup("apply");
        fs::write(dir.join("a.txt"), "one").unwrap();
        commit_all(&repo, "base");
        fs::write(dir.join("a.txt"), "two").unwrap();
        fs::write(dir.join("new.txt"), "untracked").unwrap();
        let id = save_stash(&mut repo, Some("keep me")).unwrap().to_string();
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "one");

        apply_stash(&mut repo, &id, false).unwrap();
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "two");
        assert_eq!(fs::read_to_string(dir.join("new.txt")).unwrap(), "untracked");
        let list = list_stashes(&mut repo).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);

        // With changes in the way nothing is applied; the stash stays.
        let e = apply_stash(&mut repo, &id, false).unwrap_err();
        assert!(e.contains("uncommitted changes"), "{e}");
        assert_eq!(list_stashes(&mut repo).unwrap().len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn drop_all_removes_every_stash_and_leaves_the_working_directory_alone() {
        let (dir, mut repo) = setup("dropall");
        fs::write(dir.join("a.txt"), "one").unwrap();
        commit_all(&repo, "base");
        assert_eq!(drop_all_stashes(&mut repo).unwrap(), 0);

        for (content, message) in [("two", "first"), ("three", "second"), ("four", "third")] {
            fs::write(dir.join("a.txt"), content).unwrap();
            save_stash(&mut repo, Some(message)).unwrap();
        }
        assert_eq!(list_stashes(&mut repo).unwrap().len(), 3);

        fs::write(dir.join("a.txt"), "work in progress").unwrap();
        assert_eq!(drop_all_stashes(&mut repo).unwrap(), 3);
        assert!(list_stashes(&mut repo).unwrap().is_empty());
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "work in progress");

        let _ = fs::remove_dir_all(&dir);
    }
}
