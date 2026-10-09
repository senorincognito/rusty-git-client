//! "Clean up merged": the branches whose work is already contained in the main branch.
//!
//! A branch is merged when its tip is a strict ancestor of the tip of main (so nothing on it is lost) and it is not
//! a branch that should stay. Brand-new branches that still sit exactly on main's tip are left alone: they are just as
//! likely to be a branch about to be worked on as one that was merged by fast-forward.

use git2::{BranchType, Oid, Repository};
use serde::{Deserialize, Serialize};

/// Branch names that are never offered, whatever they contain.
const PROTECTED: [&str; 5] = ["main", "master", "develop", "dev", "trunk"];

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MergedBranch {
    /// The remote for a remote branch; None for a local one.
    pub remote: Option<String>,
    pub name: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MergedBranches {
    /// The branches merged into, as shown to the user ("main", "origin/main"). Empty: there is no main branch.
    pub bases: Vec<String>,
    pub branches: Vec<MergedBranch>,
}

struct Base {
    /// "main", or "origin/main" for a remote one.
    label: String,
    /// The branch name without a remote ("main").
    short: String,
    tip: Oid,
}

fn ref_tip(repo: &Repository, full: &str) -> Option<Oid> {
    repo.find_reference(full).ok()?.peel_to_commit().ok().map(|c| c.id())
}

/// The local main branch: `main`, else `master`.
fn local_base(repo: &Repository) -> Option<Base> {
    ["main", "master"].into_iter().find_map(|n| {
        ref_tip(repo, &format!("refs/heads/{n}")).map(|tip| Base { label: n.to_string(), short: n.to_string(), tip })
    })
}

/// A remote's main branch: what `<remote>/HEAD` points at, else `<remote>/main`, else `<remote>/master`.
fn remote_base(repo: &Repository, remote: &str) -> Option<Base> {
    let prefix = format!("refs/remotes/{remote}/");
    let from_head = repo
        .find_reference(&format!("{prefix}HEAD"))
        .ok()
        .and_then(|h| h.symbolic_target().ok().flatten().map(str::to_string))
        .and_then(|target| {
            let short = target.strip_prefix(&prefix)?.to_string();
            ref_tip(repo, &target).map(|tip| (short, tip))
        });
    let (short, tip) = from_head.or_else(|| {
        ["main", "master"].into_iter().find_map(|n| ref_tip(repo, &format!("{prefix}{n}")).map(|t| (n.to_string(), t)))
    })?;
    Some(Base { label: format!("{remote}/{short}"), short, tip })
}

fn all_remotes(repo: &Repository) -> Vec<String> {
    repo.remotes().map(|r| r.iter().flatten().flatten().map(str::to_string).collect()).unwrap_or_default()
}

fn is_protected(name: &str, bases: &[Base]) -> bool {
    PROTECTED.contains(&name) || bases.iter().any(|b| b.short == name)
}

/// `tip` is an ancestor of, and not the same as, the base's tip.
fn merged_into(repo: &Repository, tip: Oid, base: Oid) -> bool {
    repo.graph_descendant_of(base, tip).unwrap_or(false)
}

/// Local branches merged into the local main branch or into the main branch of any remote (what is on a server is
/// not lost either). The checked-out branch is never listed.
fn merged_local(repo: &Repository) -> Result<MergedBranches, String> {
    let mut bases: Vec<Base> = local_base(repo).into_iter().collect();
    bases.extend(all_remotes(repo).iter().filter_map(|r| remote_base(repo, r)));
    let mut branches = Vec::new();
    for item in repo.branches(Some(BranchType::Local)).map_err(err)? {
        let (branch, _) = item.map_err(err)?;
        let (Ok(Some(name)), Some(tip)) = (branch.name(), branch.get().target()) else { continue };
        if branch.is_head() || is_protected(name, &bases) {
            continue;
        }
        if bases.iter().any(|b| merged_into(repo, tip, b.tip)) {
            branches.push(MergedBranch { remote: None, name: name.to_string() });
        }
    }
    branches.sort_by_key(|b| b.name.to_lowercase());
    Ok(MergedBranches { bases: bases.into_iter().map(|b| b.label).collect(), branches })
}

/// The remote branch the checked-out branch tracks ("origin/x"), which must not be deleted from under it.
fn head_upstream(repo: &Repository) -> Option<String> {
    let head = repo.head().ok()?;
    let branch = repo.find_branch(head.shorthand().ok()?, BranchType::Local).ok()?;
    let upstream = branch.upstream().ok()?;
    let name = upstream.name().ok().flatten().map(str::to_string);
    name
}

/// Remote branches merged into their own remote's main branch.
fn merged_remote(repo: &Repository) -> Result<MergedBranches, String> {
    let tracked = head_upstream(repo);
    let mut bases = Vec::new();
    let mut branches = Vec::new();
    for remote in all_remotes(repo) {
        let Some(base) = remote_base(repo, &remote) else { continue };
        let prefix = format!("{remote}/");
        for item in repo.branches(Some(BranchType::Remote)).map_err(err)? {
            let (branch, _) = item.map_err(err)?;
            let (Ok(Some(full)), Some(tip)) = (branch.name(), branch.get().target()) else { continue };
            let Some(name) = full.strip_prefix(&prefix) else { continue };
            if name == "HEAD"
                || is_protected(name, std::slice::from_ref(&base))
                || tracked.as_deref() == Some(full)
                || !merged_into(repo, tip, base.tip)
            {
                continue;
            }
            branches.push(MergedBranch { remote: Some(remote.clone()), name: name.to_string() });
        }
        bases.push(base.label);
    }
    branches.sort_by(|a, b| (&a.remote, a.name.to_lowercase()).cmp(&(&b.remote, b.name.to_lowercase())));
    Ok(MergedBranches { bases, branches })
}

/// Deletes those of the named local branches that are still merged.
fn delete_local(repo: &Repository, names: &[String]) -> Result<Vec<String>, String> {
    let allowed = merged_local(repo)?.branches;
    let mut deleted = Vec::new();
    for name in names {
        if !allowed.iter().any(|b| b.remote.is_none() && &b.name == name) {
            continue;
        }
        let mut branch = repo.find_branch(name, BranchType::Local).map_err(err)?;
        branch.delete().map_err(err)?;
        deleted.push(name.clone());
    }
    Ok(deleted)
}

/// Deletes those of the listed remote branches that are still merged, on the servers (one push per remote).
fn delete_remote(repo_path: &str, repo: &Repository, items: &[MergedBranch]) -> Result<Vec<MergedBranch>, String> {
    let allowed = merged_remote(repo)?.branches;
    let wanted: Vec<&MergedBranch> = items.iter().filter(|i| allowed.contains(i)).collect();
    let mut deleted = Vec::new();
    for remote in all_remotes(repo) {
        let names: Vec<&str> =
            wanted.iter().filter(|i| i.remote.as_deref() == Some(&remote)).map(|i| i.name.as_str()).collect();
        if names.is_empty() {
            continue;
        }
        let mut args = vec!["push", remote.as_str(), "--delete"];
        args.extend(names.iter().copied());
        crate::toolbar::sync::run_git(repo_path, &args)?;
        deleted.extend(wanted.iter().filter(|i| i.remote.as_deref() == Some(&remote)).map(|i| (*i).clone()));
    }
    Ok(deleted)
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// What "Clean up merged" would delete: the local branches, or with `remote` the branches on the remotes.
#[tauri::command]
pub async fn get_merged_branches(path: String, remote: bool) -> Result<MergedBranches, String> {
    blocking(move || {
        let repo = Repository::discover(&path).map_err(err)?;
        if remote {
            merged_remote(&repo)
        } else {
            merged_local(&repo)
        }
    })
    .await
}

/// Deletes the named local branches that are still merged. One undo step.
#[tauri::command]
pub async fn delete_merged_local(path: String, names: Vec<String>) -> Result<Vec<String>, String> {
    let label = format!("Delete {} merged branches", names.len());
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Keep, || {
        blocking(move || delete_local(&Repository::discover(&path).map_err(err)?, &names))
    })
    .await
}

/// Deletes the listed merged branches from their remote servers. Not undoable.
#[tauri::command]
pub async fn delete_merged_remote(path: String, items: Vec<MergedBranch>) -> Result<Vec<MergedBranch>, String> {
    blocking(move || delete_remote(&path, &Repository::discover(&path).map_err(err)?, &items)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Signature;

    fn commit(repo: &Repository, msg: &str, parent: Option<Oid>) -> Oid {
        let sig = Signature::now("t", "t@example.com").unwrap();
        let tree = repo.find_tree(repo.treebuilder(None).unwrap().write().unwrap()).unwrap();
        let parents: Vec<_> = parent.map(|p| repo.find_commit(p).unwrap()).into_iter().collect();
        repo.commit(None, &sig, &sig, msg, &tree, &parents.iter().collect::<Vec<_>>()).unwrap()
    }

    /// main = a -> m2. Local: feat (checked out, behind), merged (behind), same (= main), ahead (has its own commit),
    /// develop (behind but protected). Remote origin: main, HEAD -> main, old (behind), same (= main), wip (own commit),
    /// develop (protected), feat-up (behind; the upstream of the checked-out feat).
    fn setup(name: &str) -> (std::path::PathBuf, Repository) {
        let dir = std::env::temp_dir().join(format!("gc-cleanup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();
        let a = commit(&repo, "a", None);
        let m2 = commit(&repo, "m2", Some(a));
        let x = commit(&repo, "x", Some(a));
        for (n, oid) in [("main", m2), ("feat", a), ("merged", a), ("same", m2), ("ahead", x), ("develop", a)] {
            repo.reference(&format!("refs/heads/{n}"), oid, true, "t").unwrap();
        }
        repo.set_head("refs/heads/feat").unwrap();
        repo.remote("origin", "https://example.invalid/r.git").unwrap();
        for (n, oid) in [("main", m2), ("old", a), ("same", m2), ("wip", x), ("develop", a), ("feat-up", a)] {
            repo.reference(&format!("refs/remotes/origin/{n}"), oid, true, "t").unwrap();
        }
        repo.reference_symbolic("refs/remotes/origin/HEAD", "refs/remotes/origin/main", true, "t").unwrap();
        let mut feat = repo.find_branch("feat", BranchType::Local).unwrap();
        feat.set_upstream(Some("origin/feat-up")).unwrap();
        drop(feat);
        (dir, repo)
    }

    fn names(m: &MergedBranches) -> Vec<String> {
        m.branches.iter().map(|b| b.name.clone()).collect()
    }

    #[test]
    fn local_branches_merged_into_main_are_found() {
        let (dir, repo) = setup("local");
        let m = merged_local(&repo).unwrap();
        // `feat` is checked out, `same` sits on main's tip, `ahead` has work of its own, `develop` is protected.
        assert_eq!(names(&m), ["merged"]);
        assert_eq!(m.bases, ["main", "origin/main"]);

        let deleted = delete_local(&repo, &["merged".into(), "ahead".into(), "feat".into(), "nope".into()]).unwrap();
        assert_eq!(deleted, ["merged"]);
        assert!(repo.find_branch("ahead", BranchType::Local).is_ok());
        assert!(repo.find_branch("merged", BranchType::Local).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remote_branches_merged_into_the_remote_main_are_found() {
        let (dir, repo) = setup("remote");
        let m = merged_remote(&repo).unwrap();
        // `same` = main's tip, `wip` has its own commit, `develop` is protected, `feat-up` is the upstream of the
        // checked-out branch.
        assert_eq!(names(&m), ["old"]);
        assert_eq!(m.branches[0].remote.as_deref(), Some("origin"));
        assert_eq!(m.bases, ["origin/main"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_main_branch_nothing_is_offered() {
        let dir = std::env::temp_dir().join(format!("gc-cleanup-nomain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();
        let a = commit(&repo, "a", None);
        let b = commit(&repo, "b", Some(a));
        repo.reference("refs/heads/topic", a, true, "t").unwrap();
        repo.reference("refs/heads/trunk-like", b, true, "t").unwrap();
        repo.set_head("refs/heads/trunk-like").unwrap();
        let m = merged_local(&repo).unwrap();
        assert!(m.bases.is_empty() && m.branches.is_empty());
        assert!(merged_remote(&repo).unwrap().branches.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
