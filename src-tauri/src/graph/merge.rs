use git2::{ObjectType, Repository, RepositoryState};
use serde::Serialize;

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MergeOutcome {
    /// The checked-out branch already contains the other one; nothing happened.
    UpToDate,
    /// The checked-out branch simply moved forward to the other one (no merge commit).
    FastForward,
    /// A merge commit was created.
    Merged,
}

/// Merges `target` (a full ref name such as `refs/heads/x` or `refs/remotes/origin/x`) into the checked-out branch
/// with the system git, like `git merge --no-edit --autostash`. Uncommitted changes are set aside and restored. There
/// is no conflict screen yet, so a merge with conflicts is cancelled and the repository is left exactly as it was.
fn merge(path: &str, target: &str) -> Result<MergeOutcome, String> {
    if !target.starts_with("refs/") {
        return Err(format!("\"{target}\" is not a branch"));
    }
    let repo = Repository::discover(path).map_err(err)?;
    if repo.state() != RepositoryState::Clean {
        return Err("Finish the merge, rebase or other operation in progress first".into());
    }
    let head = repo.head().map_err(|_| "There are no commits yet".to_string())?;
    if !head.is_branch() {
        return Err("HEAD is detached. Check out a branch to merge into.".into());
    }
    let from = head.peel_to_commit().map_err(err)?.id();
    let short = target.trim_start_matches("refs/heads/").trim_start_matches("refs/remotes/");
    let to = repo
        .revparse_single(target)
        .map_err(|_| format!("Branch \"{short}\" was not found"))?
        .peel(ObjectType::Commit)
        .map_err(err)?
        .id();

    if to == from || repo.graph_descendant_of(from, to).map_err(err)? {
        return Ok(MergeOutcome::UpToDate);
    }
    let fast_forward = repo.graph_descendant_of(to, from).map_err(err)?;

    let retry = format!("git merge {short}");
    crate::toolbar::sync::run_git(path, &["merge", "--no-edit", "--autostash", target])
        .map_err(|original| crate::toolbar::sync::cancel_unfinished(path, &retry, original))?;
    Ok(if fast_forward { MergeOutcome::FastForward } else { MergeOutcome::Merged })
}

/// Merges the branch `target` (full ref name) into the checked-out branch.
#[tauri::command]
pub async fn merge_branch_cmd(path: String, target: String) -> Result<MergeOutcome, String> {
    let label = format!("Merge {}", crate::undo::short_ref(&target));
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Switch, || async move {
        tauri::async_runtime::spawn_blocking(move || merge(&path, &target)).await.map_err(|e| e.to_string())?
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{commit_file, new_repo};
    use git2::build::CheckoutBuilder;

    fn switch(repo: &Repository, branch: &str) {
        repo.set_head(&format!("refs/heads/{branch}")).unwrap();
        repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();
    }

    fn head(repo: &Repository) -> git2::Commit<'_> {
        repo.head().unwrap().peel_to_commit().unwrap()
    }

    #[test]
    fn merges_fast_forwards_and_reports_up_to_date() {
        let (dir, repo) = new_repo("merge");
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "T").unwrap();
        cfg.set_str("user.email", "t@example.com").unwrap();
        cfg.set_bool("core.autocrlf", false).unwrap();
        let p = dir.to_str().unwrap();

        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        let main = repo.head().unwrap().shorthand().unwrap().to_string();
        repo.branch("feature", &repo.find_commit(a).unwrap(), false).unwrap();
        switch(&repo, "feature");
        let g = commit_file(&repo, &dir, "g.txt", "g", "G");
        switch(&repo, &main);

        assert!(merge(p, "nope").unwrap_err().contains("not a branch"));
        assert!(merge(p, "refs/heads/missing").unwrap_err().contains("not found"));
        // main is behind feature: a plain fast-forward, no merge commit.
        assert_eq!(merge(p, "refs/heads/feature").unwrap(), MergeOutcome::FastForward);
        assert_eq!(head(&repo).id(), g);
        assert_eq!(merge(p, "refs/heads/feature").unwrap(), MergeOutcome::UpToDate);

        // Both sides have new commits: a merge commit with both as parents, all files present.
        let m = commit_file(&repo, &dir, "m.txt", "m", "M");
        switch(&repo, "feature");
        let h = commit_file(&repo, &dir, "h.txt", "h", "H");
        switch(&repo, &main);
        assert_eq!(merge(p, "refs/heads/feature").unwrap(), MergeOutcome::Merged);
        let merged = head(&repo);
        assert_eq!(merged.parent_ids().collect::<Vec<_>>(), [m, h]);
        assert!(["f.txt", "g.txt", "h.txt", "m.txt"].iter().all(|f| dir.join(f).exists()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_conflicting_merge_is_cancelled() {
        let (dir, repo) = new_repo("merge-conflict");
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "T").unwrap();
        cfg.set_str("user.email", "t@example.com").unwrap();
        cfg.set_bool("core.autocrlf", false).unwrap();
        let p = dir.to_str().unwrap();

        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        let main = repo.head().unwrap().shorthand().unwrap().to_string();
        repo.branch("other", &repo.find_commit(a).unwrap(), false).unwrap();
        switch(&repo, "other");
        commit_file(&repo, &dir, "f.txt", "theirs", "T");
        switch(&repo, &main);
        let mine = commit_file(&repo, &dir, "f.txt", "mine", "M");

        let e = merge(p, "refs/heads/other").unwrap_err();
        assert!(e.contains("conflicts") && e.contains("f.txt") && e.contains("nothing was changed"), "{e}");
        assert_eq!(repo.state(), RepositoryState::Clean);
        assert_eq!(head(&repo).id(), mine);
        assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "mine");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_detached_head_cannot_be_merged_into() {
        let (dir, repo) = new_repo("merge-detached");
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        repo.branch("other", &repo.find_commit(a).unwrap(), false).unwrap();
        repo.set_head_detached(a).unwrap();
        assert!(merge(dir.to_str().unwrap(), "refs/heads/other").unwrap_err().contains("detached"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
