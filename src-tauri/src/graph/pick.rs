use git2::{ObjectType, Repository, RepositoryState};

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

#[derive(Clone, Copy)]
enum Op {
    CherryPick,
    Revert,
}

impl Op {
    fn verb(self) -> &'static str {
        match self {
            Op::CherryPick => "cherry-pick",
            Op::Revert => "revert",
        }
    }
}

/// Applies (`CherryPick`) or reverses (`Revert`) one commit on the checked-out branch with the system git, creating a
/// new commit with the user's identity. Merge commits use their first parent as the mainline. There is no conflict
/// screen yet, so a conflicting run is aborted and the repository is left as it was.
fn pick(path: &str, id: &str, op: Op) -> Result<(), String> {
    let oid = git2::Oid::from_str(id).map_err(|_| format!("\"{id}\" is not a commit id"))?;
    let repo = Repository::discover(path).map_err(err)?;
    if repo.state() != RepositoryState::Clean {
        return Err("Finish the merge, rebase or other operation in progress first".into());
    }
    if repo.head().map(|h| !h.is_branch()).unwrap_or(false) {
        return Err(format!("HEAD is detached. Check out a branch to {} onto.", op.verb()));
    }
    let commit = repo
        .find_object(oid, None)
        .and_then(|o| o.peel(ObjectType::Commit))
        .map_err(|_| format!("Commit {id} was not found"))?
        .into_commit()
        .map_err(|_| format!("Commit {id} was not found"))?;
    if crate::sidebar::stash::stash_index_of(&repo, oid).is_some() {
        return Err("A stash can't be used here".into());
    }
    let mut args = vec![op.verb(), "--no-edit"];
    if commit.parent_count() > 1 {
        args.extend(["-m", "1"]);
    }
    let hex = commit.id().to_string();
    args.push(&hex);
    crate::toolbar::sync::run_git(path, &args).map_err(|original| cancel(path, op, &hex, original))?;
    Ok(())
}

/// After a failed run: if git stopped halfway, abort it and name the conflicting files; otherwise git's own message.
fn cancel(path: &str, op: Op, id: &str, original: String) -> String {
    let Ok(repo) = Repository::discover(path) else { return original };
    if !matches!(repo.state(), RepositoryState::CherryPick | RepositoryState::Revert)
        && !matches!(repo.state(), RepositoryState::CherryPickSequence | RepositoryState::RevertSequence)
    {
        return original;
    }
    let files = crate::toolbar::sync::run_git(path, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default();
    let files: Vec<&str> = files.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let conflicted = !files.is_empty();
    let which = if conflicted { format!(" in: {}", files.join(", ")) } else { String::new() };
    match crate::toolbar::sync::run_git(path, &[op.verb(), "--abort"]) {
        Ok(_) if conflicted => format!(
            "The {} has conflicts{which}.\n\nIt was cancelled and nothing was changed. Resolve it in the terminal with \"git {} {}\".",
            op.verb(),
            op.verb(),
            &id[..7.min(id.len())]
        ),
        Ok(_) => format!("{original}\n\nIt was cancelled and nothing was changed."),
        Err(e) => format!("{original}\n\nThe {} could not be cancelled automatically: {e}", op.verb()),
    }
}

async fn run(path: String, id: String, op: Op, label: String) -> Result<(), String> {
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Switch, || async move {
        tauri::async_runtime::spawn_blocking(move || pick(&path, &id, op)).await.map_err(|e| e.to_string())?
    })
    .await
}

/// Cherry-picks commit `id` onto the checked-out branch.
#[tauri::command]
pub async fn cherry_pick_cmd(path: String, id: String) -> Result<(), String> {
    let label = format!("Cherry-pick {}", &id[..7.min(id.len())]);
    run(path, id, Op::CherryPick, label).await
}

/// Reverts commit `id` with a new commit on the checked-out branch.
#[tauri::command]
pub async fn revert_commit_cmd(path: String, id: String) -> Result<(), String> {
    let label = format!("Revert {}", &id[..7.min(id.len())]);
    run(path, id, Op::Revert, label).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{commit_file, new_repo};
    use git2::build::CheckoutBuilder;

    fn setup(name: &str) -> (std::path::PathBuf, Repository) {
        let (dir, repo) = new_repo(name);
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "T").unwrap();
        cfg.set_str("user.email", "t@example.com").unwrap();
        cfg.set_bool("core.autocrlf", false).unwrap();
        (dir, repo)
    }

    fn switch(repo: &Repository, branch: &str) {
        repo.set_head(&format!("refs/heads/{branch}")).unwrap();
        repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();
    }

    #[test]
    fn cherry_picks_and_reverts() {
        let (dir, repo) = setup("pick");
        let p = dir.to_str().unwrap();
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        let main = repo.head().unwrap().shorthand().unwrap().to_string();
        repo.branch("feature", &repo.find_commit(a).unwrap(), false).unwrap();
        switch(&repo, "feature");
        let g = commit_file(&repo, &dir, "g.txt", "g", "G");
        switch(&repo, &main);

        assert!(pick(p, "nope", Op::CherryPick).unwrap_err().contains("not a commit id"));
        pick(p, &g.to_string(), Op::CherryPick).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("g.txt")).unwrap(), "g");
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.summary().unwrap(), Some("G"));
        assert_ne!(head.id(), g);

        pick(p, &head.id().to_string(), Op::Revert).unwrap();
        assert!(!dir.join("g.txt").exists());
        assert!(repo.head().unwrap().peel_to_commit().unwrap().summary().unwrap().unwrap().starts_with("Revert"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_conflicting_pick_is_aborted() {
        let (dir, repo) = setup("pick-conflict");
        let p = dir.to_str().unwrap();
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        let main = repo.head().unwrap().shorthand().unwrap().to_string();
        repo.branch("feature", &repo.find_commit(a).unwrap(), false).unwrap();
        switch(&repo, "feature");
        let b = commit_file(&repo, &dir, "f.txt", "b", "B");
        switch(&repo, &main);
        let c = commit_file(&repo, &dir, "f.txt", "c", "C");

        let e = pick(p, &b.to_string(), Op::CherryPick).unwrap_err();
        assert!(e.contains("conflicts") && e.contains("f.txt"), "{e}");
        assert_eq!(repo.state(), RepositoryState::Clean);
        assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), c);
        assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "c");

        // Reverting A conflicts with C's change to the same line.
        let e = pick(p, &a.to_string(), Op::Revert).unwrap_err();
        assert!(!e.is_empty());
        assert_eq!(repo.state(), RepositoryState::Clean);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
