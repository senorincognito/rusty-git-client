use git2::build::CheckoutBuilder;
use git2::{Branch, BranchType, ObjectType, Repository};
use serde::Serialize;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    /// The branch HEAD currently points at.
    pub is_head: bool,
    /// Upstream as "origin/main", if configured.
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
}

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

/// Local branches, sorted alphabetically (case-insensitive).
fn local_branches(repo: &Repository) -> Result<Vec<BranchInfo>, String> {
    let mut out = Vec::new();
    for item in repo.branches(Some(BranchType::Local)).map_err(err)? {
        let (branch, _) = item.map_err(err)?;
        let Some(name) = branch.name().ok().flatten().map(str::to_string) else { continue };

        let mut info = BranchInfo { name, is_head: branch.is_head(), upstream: None, ahead: 0, behind: 0 };
        if let Ok(up) = branch.upstream() {
            info.upstream = up.name().ok().flatten().map(str::to_string);
            if let (Some(l), Some(u)) = (branch.get().target(), up.get().target()) {
                if let Ok((ahead, behind)) = repo.graph_ahead_behind(l, u) {
                    info.ahead = ahead;
                    info.behind = behind;
                }
            }
        }
        out.push(info);
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

#[tauri::command]
pub async fn get_local_branches(path: String) -> Result<Vec<BranchInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || local_branches(&Repository::discover(&path).map_err(err)?))
        .await
        .map_err(|e| e.to_string())?
}

/// Trims `name` and checks that it can be used as a branch name.
pub(crate) fn validate_branch_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a branch name".into());
    }
    if !Branch::name_is_valid(name).map_err(err)? || name.starts_with('-') {
        return Err(format!("\"{name}\" is not a valid branch name"));
    }
    Ok(name)
}

/// Renames a local branch that is not checked out. Its upstream and branch settings move with it.
fn rename_branch(repo: &Repository, name: &str, new_name: &str) -> Result<(), String> {
    let new_name = validate_branch_name(new_name)?;
    let mut branch = repo.find_branch(name, BranchType::Local).map_err(|_| format!("Branch \"{name}\" not found"))?;
    if branch.is_head() {
        return Err("The checked-out branch can't be renamed here. Switch to another branch first.".into());
    }
    if new_name == name {
        return Err("The name is unchanged".into());
    }
    // "feature" and "feature/x" can't both exist as refs. Check this up front: libgit2 reports the
    // clash only after it has already removed the old ref (see the safety net below).
    for existing in local_branches(repo)? {
        let clash = existing.name != name
            && (existing.name.starts_with(&format!("{new_name}/"))
                || new_name.starts_with(&format!("{}/", existing.name)));
        if clash {
            return Err(format!("\"{new_name}\" conflicts with the existing branch \"{}\"", existing.name));
        }
    }

    let tip = branch.get().peel_to_commit().map_err(err)?;
    match branch.rename(new_name, false) {
        Ok(_) => Ok(()),
        Err(e) => {
            // Safety net: a failed rename must never cost the user their branch. If libgit2 dropped
            // the old ref on the way to failing, put it back.
            if repo.find_branch(name, BranchType::Local).is_err() {
                let _ = repo.branch(name, &tip, false);
            }
            if e.code() == git2::ErrorCode::Exists {
                Err(format!("A branch named \"{new_name}\" already exists"))
            } else {
                Err(err(e))
            }
        }
    }
}

/// Creates `name` at the current commit and checks it out, like `git checkout -b`.
/// The working tree and index are untouched, so uncommitted changes carry over.
fn create_and_checkout(repo: &Repository, name: &str) -> Result<(), String> {
    let name = validate_branch_name(name)?;
    let head = repo
        .head()
        .and_then(|h| h.peel_to_commit())
        .map_err(|_| "Make a first commit before creating branches".to_string())?;

    match repo.branch(name, &head, false) {
        Ok(_) => {}
        Err(e) if e.code() == git2::ErrorCode::Exists => {
            return Err(format!("A branch named \"{name}\" already exists"));
        }
        // e.g. "feature" when "feature/x" exists (or the reverse): refs can't nest like that.
        Err(e) if e.code() == git2::ErrorCode::Directory => {
            return Err(format!("\"{name}\" conflicts with an existing branch name"));
        }
        Err(e) => return Err(err(e)),
    }
    repo.set_head(&format!("refs/heads/{name}")).map_err(err)
}

#[tauri::command]
pub async fn rename_local_branch(path: String, name: String, new_name: String) -> Result<(), String> {
    let label = format!("Rename branch {name} to {}", new_name.trim());
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Keep, || async move {
        tauri::async_runtime::spawn_blocking(move || {
            rename_branch(&Repository::discover(&path).map_err(err)?, &name, &new_name)
        })
        .await
        .map_err(|e| e.to_string())?
    })
    .await
}

#[tauri::command]
pub async fn create_branch(path: String, name: String) -> Result<(), String> {
    let label = format!("Create branch {}", name.trim());
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Keep, || async move {
        tauri::async_runtime::spawn_blocking(move || {
            create_and_checkout(&Repository::discover(&path).map_err(err)?, &name)
        })
        .await
        .map_err(|e| e.to_string())?
    })
    .await
}

/// Switches to an existing local branch, like `git switch`. Safe: fails instead of
/// overwriting uncommitted changes that the target branch would touch.
fn checkout_branch(repo: &Repository, name: &str) -> Result<(), String> {
    let branch = repo.find_branch(name, BranchType::Local).map_err(|_| format!("Branch \"{name}\" not found"))?;
    if branch.is_head() {
        return Ok(());
    }
    let refname = branch.get().name().map(str::to_string).map_err(|_| "Branch name is not valid UTF-8".to_string())?;
    let target = branch.get().peel(ObjectType::Commit).map_err(err)?;

    // Update files first; HEAD moves only if that succeeded.
    let mut opts = CheckoutBuilder::new();
    opts.safe();
    repo.checkout_tree(&target, Some(&mut opts)).map_err(|e| {
        if e.code() == git2::ErrorCode::Conflict {
            "Your local changes would be overwritten by this checkout. Commit or discard them first.".to_string()
        } else {
            err(e)
        }
    })?;
    repo.set_head(&refname).map_err(err)
}

#[tauri::command]
pub async fn checkout_local_branch(path: String, name: String) -> Result<(), String> {
    let label = format!("Check out {name}");
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Switch, || async move {
        tauri::async_runtime::spawn_blocking(move || checkout_branch(&Repository::discover(&path).map_err(err)?, &name))
            .await
            .map_err(|e| e.to_string())?
    })
    .await
}

/// Checks out the remote branch `remote/name` as a local branch of the same name that tracks it, like
/// `git switch name` does. An existing local branch of that name is used when it already tracks the remote
/// branch. If the checkout fails (local changes in the way) nothing is left behind.
fn checkout_remote(repo: &Repository, remote: &str, name: &str) -> Result<(), String> {
    let full = format!("{remote}/{name}");
    let remote_branch =
        repo.find_branch(&full, BranchType::Remote).map_err(|_| format!("Remote branch \"{full}\" not found"))?;
    if let Ok(local) = repo.find_branch(name, BranchType::Local) {
        let tracks = local.upstream().ok().and_then(|u| u.name().ok().flatten().map(str::to_string));
        if tracks.as_deref() != Some(full.as_str()) {
            return Err(format!("A local branch named \"{name}\" already exists and does not track \"{full}\""));
        }
        return checkout_branch(repo, name);
    }
    let tip = remote_branch.get().peel_to_commit().map_err(err)?;
    let mut local = match repo.branch(name, &tip, false) {
        Ok(b) => b,
        // e.g. "feature" when "feature/x" exists locally (or the reverse).
        Err(e) if e.code() == git2::ErrorCode::Directory => {
            return Err(format!("\"{name}\" conflicts with an existing local branch name"));
        }
        Err(e) => return Err(err(e)),
    };
    let result = local.set_upstream(Some(&full)).map_err(err).and_then(|_| checkout_branch(repo, name));
    if result.is_err() {
        let _ = local.delete();
    }
    result
}

#[tauri::command]
pub async fn checkout_remote_branch(path: String, remote: String, name: String) -> Result<(), String> {
    let label = format!("Check out {remote}/{name}");
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Switch, || async move {
        tauri::async_runtime::spawn_blocking(move || {
            checkout_remote(&Repository::discover(&path).map_err(err)?, &remote, &name)
        })
        .await
        .map_err(|e| e.to_string())?
    })
    .await
}

/// Commits on `name` that are neither in the current HEAD nor on its upstream, i.e. work
/// that would become unreachable by deleting the branch (same rule as `git branch -d`).
fn unmerged_commits(repo: &Repository, name: &str) -> Result<usize, String> {
    let branch = repo.find_branch(name, BranchType::Local).map_err(|_| format!("Branch \"{name}\" not found"))?;
    let tip = branch.get().peel_to_commit().map_err(err)?.id();

    let mut walk = repo.revwalk().map_err(err)?;
    walk.push(tip).map_err(err)?;
    if let Ok(head) = repo.head().and_then(|h| h.peel_to_commit()) {
        walk.hide(head.id()).map_err(err)?;
    }
    if let Some(up) = branch.upstream().ok().and_then(|u| u.get().target()) {
        walk.hide(up).map_err(err)?;
    }
    Ok(walk.count())
}

fn delete_branch_checked(repo: &Repository, name: &str) -> Result<(), String> {
    let mut branch = repo.find_branch(name, BranchType::Local).map_err(|_| format!("Branch \"{name}\" not found"))?;
    if branch.is_head() {
        return Err("Cannot delete the branch that is currently checked out".into());
    }
    branch.delete().map_err(err)
}

/// Number of commits that deleting `name` would leave unreachable (0 when fully merged).
#[tauri::command]
pub async fn count_unmerged_commits(path: String, name: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || unmerged_commits(&Repository::discover(&path).map_err(err)?, &name))
        .await
        .map_err(|e| e.to_string())?
}

/// Deletes a local branch. The checked-out branch is refused.
#[tauri::command]
pub async fn delete_local_branch(path: String, name: String) -> Result<(), String> {
    let label = format!("Delete branch {name}");
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Keep, || async move {
        tauri::async_runtime::spawn_blocking(move || {
            delete_branch_checked(&Repository::discover(&path).map_err(err)?, &name)
        })
        .await
        .map_err(|e| e.to_string())?
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Signature;

    #[test]
    fn lists_branches_alphabetically_and_flags_head() {
        let dir = std::env::temp_dir().join(format!("gc-branches-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();

        // No commits yet: no branches exist.
        assert!(local_branches(&repo).unwrap().is_empty());

        let sig = Signature::now("t", "t@example.com").unwrap();
        let tree = repo.find_tree(repo.treebuilder(None).unwrap().write().unwrap()).unwrap();
        let c = repo.commit(Some("refs/heads/main"), &sig, &sig, "c", &tree, &[]).unwrap();
        repo.set_head("refs/heads/main").unwrap();
        let commit = repo.find_commit(c).unwrap();
        for name in ["zeta", "Beta", "alpha", "feature/x"] {
            repo.branch(name, &commit, false).unwrap();
        }

        let list = local_branches(&repo).unwrap();
        let names: Vec<_> = list.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["alpha", "Beta", "feature/x", "main", "zeta"]);
        assert_eq!(list.iter().filter(|b| b.is_head).map(|b| b.name.as_str()).collect::<Vec<_>>(), ["main"]);
        assert!(list.iter().all(|b| b.upstream.is_none()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_and_checkout_branch() {
        let dir = std::env::temp_dir().join(format!("gc-newbranch-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();

        // Unborn branch: nothing to branch from.
        assert!(create_and_checkout(&repo, "x").unwrap_err().contains("first commit"));

        let sig = Signature::now("t", "t@example.com").unwrap();
        let tree = repo.find_tree(repo.treebuilder(None).unwrap().write().unwrap()).unwrap();
        let c = repo.commit(Some("refs/heads/main"), &sig, &sig, "c", &tree, &[]).unwrap();
        repo.set_head("refs/heads/main").unwrap();

        assert!(create_and_checkout(&repo, "  ").is_err());
        for bad in ["a b", "a..b", "-x", "x.lock", "HEAD", "a~1", "/x"] {
            assert!(create_and_checkout(&repo, bad).is_err(), "{bad} should be rejected");
        }
        assert!(create_and_checkout(&repo, "main").unwrap_err().contains("already exists"));

        // Uncommitted work survives the checkout.
        std::fs::write(dir.join("wip.txt"), "work").unwrap();
        create_and_checkout(&repo, " feature/login ").unwrap();
        assert_eq!(repo.head().unwrap().shorthand(), Ok("feature/login"));
        assert_eq!(repo.head().unwrap().target(), Some(c));
        assert_eq!(std::fs::read_to_string(dir.join("wip.txt")).unwrap(), "work");

        // "feature" would collide with the existing "feature/login" ref directory.
        assert!(create_and_checkout(&repo, "feature").is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn commit_files(repo: &Repository, refname: &str, parents: &[git2::Oid], files: &[(&str, &str)]) -> git2::Oid {
        let sig = Signature::now("t", "t@example.com").unwrap();
        let mut tb = repo.treebuilder(None).unwrap();
        for (name, content) in files {
            tb.insert(name, repo.blob(content.as_bytes()).unwrap(), 0o100644).unwrap();
        }
        let tree = repo.find_tree(tb.write().unwrap()).unwrap();
        let ps: Vec<_> = parents.iter().map(|o| repo.find_commit(*o).unwrap()).collect();
        let refs: Vec<_> = ps.iter().collect();
        repo.commit(Some(refname), &sig, &sig, "c", &tree, &refs).unwrap()
    }

    #[test]
    fn checkout_switches_branches_safely() {
        let dir = std::env::temp_dir().join(format!("gc-checkout-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();

        let c1 = commit_files(&repo, "refs/heads/main", &[], &[("a.txt", "one")]);
        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();
        commit_files(&repo, "refs/heads/feature", &[c1], &[("a.txt", "two"), ("c.txt", "x")]);

        assert!(checkout_branch(&repo, "nope").unwrap_err().contains("not found"));
        checkout_branch(&repo, "main").unwrap(); // already current: no-op

        checkout_branch(&repo, "feature").unwrap();
        assert_eq!(repo.head().unwrap().shorthand(), Ok("feature"));
        assert_eq!((read("a.txt").as_deref(), read("c.txt").as_deref()), (Some("two"), Some("x")));

        checkout_branch(&repo, "main").unwrap();
        assert_eq!(repo.head().unwrap().shorthand(), Ok("main"));
        assert_eq!((read("a.txt").as_deref(), read("c.txt")), (Some("one"), None));

        // A change to a file the other branch also changes blocks the switch.
        std::fs::write(dir.join("a.txt"), "local edit").unwrap();
        let e = checkout_branch(&repo, "feature").unwrap_err();
        assert!(e.contains("overwritten"), "{e}");
        assert_eq!(repo.head().unwrap().shorthand(), Ok("main"));
        assert_eq!(read("a.txt").as_deref(), Some("local edit"));

        // Unrelated untracked files simply come along.
        std::fs::write(dir.join("a.txt"), "one").unwrap();
        std::fs::write(dir.join("notes.txt"), "mine").unwrap();
        checkout_branch(&repo, "feature").unwrap();
        assert_eq!(read("notes.txt").as_deref(), Some("mine"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_branch_rules() {
        let dir = std::env::temp_dir().join(format!("gc-delbranch-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();

        let c1 = commit_files(&repo, "refs/heads/main", &[], &[("a.txt", "one")]);
        repo.set_head("refs/heads/main").unwrap();
        let c2 = commit_files(&repo, "refs/heads/feature", &[c1], &[("a.txt", "two")]);
        commit_files(&repo, "refs/heads/feature", &[c2], &[("a.txt", "three")]);
        repo.branch("merged", &repo.find_commit(c1).unwrap(), false).unwrap();

        assert_eq!(unmerged_commits(&repo, "merged").unwrap(), 0);
        assert_eq!(unmerged_commits(&repo, "feature").unwrap(), 2);
        assert_eq!(unmerged_commits(&repo, "main").unwrap(), 0);
        assert!(unmerged_commits(&repo, "nope").is_err());

        // Commits already on the upstream don't count as lost.
        repo.remote("origin", "https://example.com/r.git").unwrap();
        let tip = repo.find_branch("feature", BranchType::Local).unwrap().get().target().unwrap();
        repo.reference("refs/remotes/origin/feature", tip, true, "test").unwrap();
        repo.find_branch("feature", BranchType::Local).unwrap().set_upstream(Some("origin/feature")).unwrap();
        assert_eq!(unmerged_commits(&repo, "feature").unwrap(), 0);

        // The checked-out branch can't be deleted; others can.
        assert!(delete_branch_checked(&repo, "main").unwrap_err().contains("checked out"));
        delete_branch_checked(&repo, "merged").unwrap();
        delete_branch_checked(&repo, "feature").unwrap();
        let names: Vec<_> = local_branches(&repo).unwrap().into_iter().map(|b| b.name).collect();
        assert_eq!(names, ["main"]);
        assert!(delete_branch_checked(&repo, "merged").unwrap_err().contains("not found"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_local_branch_rules() {
        let dir = std::env::temp_dir().join(format!("gc-renamebranch-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();
        let c1 = commit_files(&repo, "refs/heads/main", &[], &[("a.txt", "one")]);
        repo.set_head("refs/heads/main").unwrap();
        let commit = repo.find_commit(c1).unwrap();
        repo.branch("other", &commit, false).unwrap();
        repo.branch("taken", &commit, false).unwrap();
        repo.branch("feature/x", &commit, false).unwrap();

        // Give "other" an upstream to see that it moves with the branch.
        repo.remote("origin", "https://example.com/r.git").unwrap();
        repo.reference("refs/remotes/origin/other", c1, true, "test").unwrap();
        repo.find_branch("other", BranchType::Local).unwrap().set_upstream(Some("origin/other")).unwrap();

        // Rules: not the checked-out branch, valid and free names only.
        assert!(rename_branch(&repo, "main", "primary").unwrap_err().contains("checked-out"));
        assert!(rename_branch(&repo, "nope", "x").unwrap_err().contains("not found"));
        assert!(rename_branch(&repo, "other", "taken").unwrap_err().contains("already exists"));
        assert!(rename_branch(&repo, "other", "other").unwrap_err().contains("unchanged"));
        assert!(rename_branch(&repo, "other", "  ").is_err());
        for bad in ["a b", "-x", "a..b", "x.lock"] {
            assert!(rename_branch(&repo, "other", bad).is_err(), "{bad} should be rejected");
        }
        // Collisions with feature/x are refused up front, and the branch must survive them.
        for clash in ["feature", "feature/x/y"] {
            let e = rename_branch(&repo, "other", clash).unwrap_err();
            assert!(e.contains("conflicts"), "{clash}: {e}");
            assert!(repo.find_branch("other", BranchType::Local).is_ok(), "{clash} must not delete other");
        }
        // Renaming a branch into its own namespace is not a clash with itself.
        assert!(rename_branch(&repo, "feature/x", "feature/y").is_ok());
        rename_branch(&repo, "feature/y", "feature/x").unwrap();

        rename_branch(&repo, "other", " renamed ").unwrap();
        assert!(repo.find_branch("other", BranchType::Local).is_err());
        let renamed = repo.find_branch("renamed", BranchType::Local).unwrap();
        assert_eq!(renamed.get().target(), Some(c1));
        assert_eq!(renamed.upstream().unwrap().name(), Ok(Some("origin/other")));
        assert_eq!(repo.head().unwrap().shorthand(), Ok("main"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn checks_out_a_remote_branch_as_a_tracking_branch() {
        let dir = std::env::temp_dir().join(format!("gc-remote-checkout-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();
        let sig = Signature::now("t", "t@example.com").unwrap();
        let tree = repo.find_tree(repo.treebuilder(None).unwrap().write().unwrap()).unwrap();
        let c = repo.commit(Some("refs/heads/main"), &sig, &sig, "c", &tree, &[]).unwrap();
        repo.set_head("refs/heads/main").unwrap();
        // A remote-tracking ref without a server is enough: no network is involved.
        repo.remote("origin", "https://example.invalid/r.git").unwrap();
        repo.reference("refs/remotes/origin/feature/x", c, true, "test").unwrap();

        assert!(checkout_remote(&repo, "origin", "nope").unwrap_err().contains("not found"));
        checkout_remote(&repo, "origin", "feature/x").unwrap();
        let list = local_branches(&repo).unwrap();
        let x = list.iter().find(|b| b.name == "feature/x").unwrap();
        assert!(x.is_head);
        assert_eq!(x.upstream.as_deref(), Some("origin/feature/x"));

        // Again: the tracking branch already exists and is simply checked out.
        checkout_branch(&repo, "main").unwrap();
        checkout_remote(&repo, "origin", "feature/x").unwrap();
        assert!(local_branches(&repo).unwrap().iter().find(|b| b.name == "feature/x").unwrap().is_head);

        // A same-named local branch that tracks nothing is not silently taken over.
        repo.reference("refs/remotes/origin/main", c, true, "test").unwrap();
        assert!(checkout_remote(&repo, "origin", "main").unwrap_err().contains("does not track"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
