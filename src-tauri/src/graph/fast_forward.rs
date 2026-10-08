use git2::build::CheckoutBuilder;
use git2::{ObjectType, Repository, RepositoryState};

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

/// Moves the checked-out branch forward to `target` (a commit id or a full ref name such as `refs/heads/x`), like
/// `git merge --ff-only`. The target must be a descendant of the current commit. The working directory is updated
/// safely first: if uncommitted changes are in the way nothing moves. Returns how many commits the branch gained.
fn fast_forward(repo: &Repository, target: &str) -> Result<usize, String> {
    if repo.state() != RepositoryState::Clean {
        return Err("Finish the merge, rebase or other operation in progress first".into());
    }
    let head = repo.head().map_err(|_| "There are no commits yet".to_string())?;
    if !head.is_branch() {
        return Err("HEAD is detached. Check out a branch to fast-forward it.".into());
    }
    let refname = head.name().map_err(|_| "Branch name is not valid UTF-8".to_string())?.to_string();
    let branch = head.shorthand().unwrap_or("the current branch").to_string();
    let from = head.peel_to_commit().map_err(err)?.id();

    let object = repo.revparse_single(target).map_err(|_| format!("\"{target}\" was not found"))?;
    let to = object.peel(ObjectType::Commit).map_err(err)?.id();
    if crate::sidebar::stash::stash_index_of(repo, to).is_some() {
        return Err("A stash can't be fast-forwarded to".into());
    }

    if to == from || repo.graph_descendant_of(from, to).map_err(err)? {
        return Err(format!("{branch} already contains this commit"));
    }
    if !repo.graph_descendant_of(to, from).map_err(err)? {
        let (ahead, behind) = repo.graph_ahead_behind(from, to).map_err(err)?;
        return Err(format!(
            "{branch} can't be fast-forwarded: it has {ahead} commit{} that the target does not have, and the target has \
             {behind} that {branch} does not have. Merge or rebase instead.",
            if ahead == 1 { "" } else { "s" }
        ));
    }
    let gained = repo.graph_ahead_behind(to, from).map_err(err)?.0;

    // Files first; the branch moves only if that worked.
    let commit_obj = repo.find_object(to, Some(ObjectType::Commit)).map_err(err)?;
    let mut opts = CheckoutBuilder::new();
    opts.safe();
    repo.checkout_tree(&commit_obj, Some(&mut opts)).map_err(|e| {
        if e.code() == git2::ErrorCode::Conflict {
            "Your local changes would be overwritten by this fast-forward. Commit, stash or discard them first."
                .to_string()
        } else {
            err(e)
        }
    })?;
    repo.reference(&refname, to, true, "fast-forward").map_err(err)?;
    Ok(gained)
}

/// Fast-forwards the checked-out branch to `target`; resolves to the number of commits gained.
#[tauri::command]
pub async fn fast_forward_cmd(path: String, target: String) -> Result<usize, String> {
    let label = format!("Fast-forward to {}", crate::undo::short_ref(&target));
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Switch, || async move {
        tauri::async_runtime::spawn_blocking(move || fast_forward(&Repository::discover(&path).map_err(err)?, &target))
            .await
            .map_err(|e| e.to_string())?
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{commit_file, new_repo};
    use git2::Oid;

    fn head_id(repo: &Repository) -> Oid {
        repo.head().unwrap().peel_to_commit().unwrap().id()
    }

    #[test]
    fn fast_forwards_the_checked_out_branch() {
        let (dir, repo) = new_repo("ff");
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        let main_ref = repo.head().unwrap().name().unwrap().to_string();
        repo.branch("feature", &repo.find_commit(a).unwrap(), false).unwrap();
        // feature goes ahead by two commits, then main is checked out again at A.
        repo.set_head("refs/heads/feature").unwrap();
        commit_file(&repo, &dir, "f.txt", "b", "B");
        let c = commit_file(&repo, &dir, "g.txt", "c", "C");
        repo.set_head(&main_ref).unwrap();
        repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();
        assert_eq!(head_id(&repo), a);

        assert!(fast_forward(&repo, "nope").unwrap_err().contains("not found"));
        assert_eq!(fast_forward(&repo, "refs/heads/feature").unwrap(), 2);
        assert_eq!(head_id(&repo), c);
        assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "b");
        assert!(dir.join("g.txt").exists());
        // Already there, and the other way round (feature is behind nothing, a is an ancestor).
        assert!(fast_forward(&repo, &c.to_string()).unwrap_err().contains("already contains"));
        assert!(fast_forward(&repo, &a.to_string()).unwrap_err().contains("already contains"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_diverged_branches_and_dirty_conflicts() {
        let (dir, repo) = new_repo("ff-refuse");
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        let main_ref = repo.head().unwrap().name().unwrap().to_string();
        repo.branch("feature", &repo.find_commit(a).unwrap(), false).unwrap();
        repo.set_head("refs/heads/feature").unwrap();
        let b = commit_file(&repo, &dir, "f.txt", "b", "B");
        repo.set_head(&main_ref).unwrap();
        repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();

        // A local edit that the fast-forward would overwrite stops it, and the branch stays.
        std::fs::write(dir.join("f.txt"), "mine").unwrap();
        let e = fast_forward(&repo, "refs/heads/feature").unwrap_err();
        assert!(e.contains("local changes"), "{e}");
        assert_eq!(head_id(&repo), a);
        assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "mine");
        repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();

        // main gets its own commit: the histories diverge.
        let m = commit_file(&repo, &dir, "other.txt", "m", "M");
        let e = fast_forward(&repo, &b.to_string()).unwrap_err();
        assert!(e.contains("can't be fast-forwarded"), "{e}");
        assert_eq!(head_id(&repo), m);

        // A detached HEAD has no branch to move.
        repo.set_head_detached(m).unwrap();
        assert!(fast_forward(&repo, &b.to_string()).unwrap_err().contains("detached"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
