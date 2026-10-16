//! Rebases the checked-out branch onto another branch (`git rebase <branch>`): its own commits are replayed on top
//! of the other branch, with new ids. The system git does the work. There is no conflict screen yet, so a rebase with
//! conflicts is cancelled and the repository is left exactly as it was.

use git2::{ObjectType, Oid, Repository, RepositoryState, Sort};
use serde::Serialize;

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RebaseOutcome {
    /// The checked-out branch already contains the other one; nothing happened.
    UpToDate,
    /// The checked-out branch had no commits of its own and simply moved forward.
    FastForward,
    /// The branch's own commits were replayed on top of the other branch.
    Rebased,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RebaseOntoInfo {
    /// The checked-out branch.
    pub branch: String,
    /// Commits on it that the target does not have: these are replayed.
    pub ahead: usize,
    /// Commits on the target that the branch does not have.
    pub behind: usize,
    /// How many of the replayed commits are already on the branch's upstream (a force push is needed afterwards).
    pub pushed: usize,
    /// Merge commits among the replayed ones: a plain rebase flattens them.
    pub merges: usize,
}

/// HEAD's branch name and commit, and the target's commit, after the checks every rebase needs.
fn prepare(repo: &Repository, target: &str) -> Result<(String, Oid, Oid), String> {
    if !target.starts_with("refs/") {
        return Err(format!("\"{target}\" is not a branch"));
    }
    if repo.state() != RepositoryState::Clean {
        return Err("Finish the merge, rebase or other operation in progress first".into());
    }
    let head = repo.head().map_err(|_| "There are no commits yet".to_string())?;
    if !head.is_branch() {
        return Err("HEAD is detached. Check out a branch to rebase it.".into());
    }
    let branch = head.shorthand().map_err(|_| "Branch name is not valid UTF-8".to_string())?.to_string();
    let from = head.peel_to_commit().map_err(err)?.id();
    let to = repo
        .revparse_single(target)
        .map_err(|_| format!("Branch \"{}\" was not found", crate::undo::short_ref(target)))?
        .peel(ObjectType::Commit)
        .map_err(err)?
        .id();
    Ok((branch, from, to))
}

/// What a rebase onto `target` would replay, for the confirmation.
fn info(repo: &Repository, target: &str) -> Result<RebaseOntoInfo, String> {
    let (branch, from, to) = prepare(repo, target)?;
    let (ahead, behind) = repo.graph_ahead_behind(from, to).map_err(err)?;
    let mut walk = repo.revwalk().map_err(err)?;
    walk.set_sorting(Sort::TOPOLOGICAL).map_err(err)?;
    walk.push(from).map_err(err)?;
    walk.hide(to).map_err(err)?;
    let (mut pushed, mut merges) = (0, 0);
    for oid in walk.flatten() {
        if crate::history::is_pushed(repo, oid) {
            pushed += 1;
        }
        if repo.find_commit(oid).map_err(err)?.parent_count() > 1 {
            merges += 1;
        }
    }
    Ok(RebaseOntoInfo { branch, ahead, behind, pushed, merges })
}

/// Rebases the checked-out branch onto `target` (a full ref name) with `git rebase --autostash`.
fn rebase(path: &str, target: &str) -> Result<RebaseOutcome, String> {
    let repo = Repository::discover(path).map_err(err)?;
    let (_, from, to) = prepare(&repo, target)?;
    if to == from || repo.graph_descendant_of(from, to).map_err(err)? {
        return Ok(RebaseOutcome::UpToDate);
    }
    let fast_forward = repo.graph_descendant_of(to, from).map_err(err)?;

    let retry = format!("git rebase {}", crate::undo::short_ref(target));
    crate::toolbar::sync::run_git(path, &["rebase", "--autostash", target])
        .map_err(|original| crate::toolbar::sync::cancel_unfinished(path, &retry, original))?;
    Ok(if fast_forward { RebaseOutcome::FastForward } else { RebaseOutcome::Rebased })
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// What rebasing the checked-out branch onto `target` would replay.
#[tauri::command]
pub async fn get_rebase_onto_info(path: String, target: String) -> Result<RebaseOntoInfo, String> {
    blocking(move || info(&Repository::discover(&path).map_err(err)?, &target)).await
}

/// Rebases the checked-out branch onto the branch `target` (full ref name).
#[tauri::command]
pub async fn rebase_onto_cmd(path: String, target: String) -> Result<RebaseOutcome, String> {
    let label = format!("Rebase onto {}", crate::undo::short_ref(&target));
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Switch, || blocking(move || rebase(&path, &target)))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{commit_file, new_repo};
    use git2::build::CheckoutBuilder;
    use std::fs;
    use std::path::Path;

    fn switch(repo: &Repository, branch: &str) {
        repo.set_head(&format!("refs/heads/{branch}")).unwrap();
        repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();
    }

    fn head(repo: &Repository) -> git2::Commit<'_> {
        repo.head().unwrap().peel_to_commit().unwrap()
    }

    fn configure(repo: &Repository) {
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "T").unwrap();
        cfg.set_str("user.email", "t@example.com").unwrap();
        cfg.set_bool("core.autocrlf", false).unwrap();
    }

    #[test]
    fn replays_the_branch_on_top_of_the_target() {
        let (dir, repo) = new_repo("rebase-onto");
        configure(&repo);
        let p = dir.to_str().unwrap();
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        let main = repo.head().unwrap().shorthand().unwrap().to_string();
        repo.branch("feature", &repo.find_commit(a).unwrap(), false).unwrap();
        repo.branch("old", &repo.find_commit(a).unwrap(), false).unwrap();
        switch(&repo, "feature");
        commit_file(&repo, &dir, "g.txt", "g", "G");
        switch(&repo, &main);
        let h = commit_file(&repo, &dir, "h.txt", "h", "H");
        switch(&repo, "feature");
        let main_ref = format!("refs/heads/{main}");

        assert!(rebase(p, "nope").unwrap_err().contains("not a branch"));
        let i = info(&repo, &main_ref).unwrap();
        assert_eq!((i.branch.as_str(), i.ahead, i.behind, i.pushed, i.merges), ("feature", 1, 1, 0, 0));

        assert_eq!(rebase(p, &main_ref).unwrap(), RebaseOutcome::Rebased);
        let tip = head(&repo);
        assert_eq!(tip.parent_id(0).unwrap(), h);
        assert_eq!(tip.message().unwrap().trim(), "G");
        assert!(dir.join("g.txt").exists() && dir.join("h.txt").exists());

        // Already based on it.
        assert_eq!(rebase(p, &main_ref).unwrap(), RebaseOutcome::UpToDate);

        // A branch with no commits of its own just moves forward.
        switch(&repo, "old");
        assert_eq!(rebase(p, &main_ref).unwrap(), RebaseOutcome::FastForward);
        assert_eq!(head(&repo).id(), h);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_conflicting_rebase_is_cancelled_and_nothing_changes() {
        let (dir, repo) = new_repo("rebase-conflict");
        configure(&repo);
        let p = dir.to_str().unwrap();
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        let main = repo.head().unwrap().shorthand().unwrap().to_string();
        repo.branch("topic", &repo.find_commit(a).unwrap(), false).unwrap();
        switch(&repo, "topic");
        let mine = commit_file(&repo, &dir, "f.txt", "mine", "mine");
        switch(&repo, &main);
        commit_file(&repo, &dir, "f.txt", "theirs", "theirs");
        switch(&repo, "topic");

        let e = rebase(p, &format!("refs/heads/{main}")).unwrap_err();
        assert!(e.contains("conflicts") && e.contains("f.txt") && e.contains("nothing was changed"), "{e}");
        assert_eq!(repo.state(), RepositoryState::Clean);
        assert_eq!(head(&repo).id(), mine);
        assert_eq!(fs::read_to_string(dir.join("f.txt")).unwrap(), "mine");
        assert!(!Path::new(&dir.join(".git").join("rebase-merge")).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_detached_head_cannot_be_rebased() {
        let (dir, repo) = new_repo("rebase-detached");
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        repo.branch("other", &repo.find_commit(a).unwrap(), false).unwrap();
        repo.set_head_detached(a).unwrap();
        assert!(info(&repo, "refs/heads/other").unwrap_err().contains("detached"));
        let _ = fs::remove_dir_all(&dir);
    }
}
