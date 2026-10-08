//! Drop a commit from the current branch: the later commits are replayed without it.

use git2::{Commit, Oid, Repository, RepositoryState, ResetType};
use serde::Serialize;

use super::{blocking, err, is_pushed};

/// What dropping a commit involves, after checking that it is possible: the commit, its first
/// parent, and the commits after it (oldest first) that have to be re-created on top of that parent.
struct DropPlan<'r> {
    target: Commit<'r>,
    parent: Commit<'r>,
    later: Vec<Commit<'r>>,
}

/// Only commits on the branch's own line of history can be dropped, and only while the commits after
/// them form a straight line: replaying a merge commit is ambiguous, so it is not attempted.
fn drop_plan<'r>(repo: &'r Repository, id: &str) -> Result<DropPlan<'r>, String> {
    if repo.state() != RepositoryState::Clean {
        return Err("Finish the merge, rebase or other operation in progress first".into());
    }
    let head = repo.head().map_err(|_| "There are no commits yet".to_string())?;
    if !head.is_branch() {
        return Err("Check out a branch first: a detached HEAD has no branch to rewrite".into());
    }
    let target_id = Oid::from_str(id).map_err(err)?;
    let not_droppable = "Only commits on the current branch's own line of history, with no merge commit after them, \
                         can be dropped";

    let mut later: Vec<Commit<'r>> = Vec::new();
    let mut cursor = head.peel_to_commit().map_err(err)?;
    while cursor.id() != target_id {
        if cursor.parent_count() != 1 {
            return Err(not_droppable.into());
        }
        let parent = cursor.parent(0).map_err(err)?;
        later.push(cursor);
        cursor = parent;
    }
    let parent = cursor.parent(0).map_err(|_| "The first commit of a branch can't be dropped".to_string())?;
    later.reverse();
    Ok(DropPlan { target: cursor, parent, later })
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DropInfo {
    pub short_id: String,
    pub summary: String,
    /// The commit itself is already on the upstream: dropping it rewrites published history.
    pub pushed: bool,
    /// A merge commit: the branch goes back to its first parent.
    pub is_merge: bool,
    /// Commits after it on the branch, which are re-created (new ids) on top of its parent.
    pub later_commits: usize,
    /// How many of those are already on the upstream.
    pub later_pushed: usize,
}

fn drop_info(repo: &Repository, id: &str) -> Result<DropInfo, String> {
    let plan = drop_plan(repo, id)?;
    Ok(DropInfo {
        short_id: plan.target.id().to_string()[..7].to_string(),
        summary: plan.target.summary().ok().flatten().unwrap_or("").to_string(),
        pushed: is_pushed(repo, plan.target.id()),
        is_merge: plan.target.parent_count() > 1,
        later_commits: plan.later.len(),
        later_pushed: plan.later.iter().filter(|c| is_pushed(repo, c.id())).count(),
    })
}

/// Removes a commit from the current branch. The commits after it are re-created on top of its
/// parent by applying each one's changes (a 3-way merge, same author, message and dates), all in
/// memory: if any of them depends on the dropped commit and would conflict, nothing is changed.
/// Only when every commit applies cleanly is the branch moved, and the index and working directory
/// with it (`git reset --hard`). That is why the working directory must be clean: the commit's
/// changes leave the files on disk, and uncommitted work would be lost with them. The old commits
/// stay in the reflog for a while.
fn drop_commit(repo: &Repository, id: &str) -> Result<(), String> {
    let plan = drop_plan(repo, id)?;
    if !crate::changes::status_of(repo)?.is_empty() {
        return Err("The working directory has uncommitted changes. Commit or stash them first: dropping a commit \
                    resets the files to the new history."
            .into());
    }

    let mut new_tip = plan.parent.clone();
    for commit in &plan.later {
        let mut merged = repo.cherrypick_commit(commit, &new_tip, 0, None).map_err(err)?;
        if merged.has_conflicts() {
            let files: Vec<String> = merged
                .conflicts()
                .map_err(err)?
                .flatten()
                .filter_map(|c| c.our.or(c.their).or(c.ancestor))
                .map(|e| String::from_utf8_lossy(&e.path).into_owned())
                .collect();
            return Err(format!(
                "Dropping this commit would conflict: the later commit {} depends on it ({}). Nothing was changed.",
                &commit.id().to_string()[..7],
                files.join(", ")
            ));
        }
        let tree = repo.find_tree(merged.write_tree_to(repo).map_err(err)?).map_err(err)?;
        let message = String::from_utf8_lossy(commit.message_raw_bytes()).into_owned();
        let rebuilt =
            repo.commit(None, &commit.author(), &commit.committer(), &message, &tree, &[&new_tip]).map_err(err)?;
        new_tip = repo.find_commit(rebuilt).map_err(err)?;
    }

    repo.reset(new_tip.as_object(), ResetType::Hard, None).map_err(err)
}

/// What a "drop commit" would remove, for the confirmation dialog.
#[tauri::command]
pub async fn get_drop_info(path: String, id: String) -> Result<DropInfo, String> {
    blocking(path, move |r| drop_info(r, &id)).await
}

/// Drops the current branch's latest commit (and its changes). Needs a clean working directory.
#[tauri::command]
pub async fn drop_latest_commit(path: String, id: String) -> Result<(), String> {
    crate::undo::recorded(&path.clone(), "Drop commit", crate::undo::Kind::Switch, || {
        blocking(path, move |r| drop_commit(r, &id))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{commit_file, log, merge_commit, new_repo};
    use git2::{BranchType, Signature};

    #[test]
    fn dropping_the_tip_and_older_commits() {
        let (dir, repo) = new_repo("any");
        let a = commit_file(&repo, &dir, "a.txt", "one", "a");
        let b = commit_file(&repo, &dir, "b.txt", "bee", "b");
        let c = commit_file(&repo, &dir, "c.txt", "sea", "c");
        let d = commit_file(&repo, &dir, "d.txt", "dee", "d");

        // The plan describes what a drop involves.
        let info = drop_info(&repo, &b.to_string()).unwrap();
        assert_eq!((info.summary.as_str(), info.later_commits, info.pushed, info.is_merge), ("b", 2, false, false));
        assert_eq!(drop_info(&repo, &d.to_string()).unwrap().later_commits, 0);

        // Dropping the middle commit b: c and d are replayed (new ids, same content), b's file is gone.
        drop_commit(&repo, &b.to_string()).unwrap();
        assert_eq!(log(&repo), ["d", "c", "a"]);
        assert!(!dir.join("b.txt").exists());
        assert_eq!(std::fs::read_to_string(dir.join("c.txt")).unwrap(), "sea");
        assert_eq!(std::fs::read_to_string(dir.join("d.txt")).unwrap(), "dee");
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_ne!(head.id(), d, "the later commits were re-created");
        assert_eq!(head.author().name(), Ok("D"));
        assert_eq!(head.message(), Ok("d"));
        assert_eq!(head.parent(0).unwrap().parent_id(0).unwrap(), a);
        assert!(crate::changes::status_of(&repo).unwrap().is_empty());
        assert!(repo.find_commit(b).is_ok(), "the dropped commit stays in the object database (reflog)");

        // Dropping the tip is the simple case: no later commits.
        let tip = repo.head().unwrap().peel_to_commit().unwrap().id();
        drop_commit(&repo, &tip.to_string()).unwrap();
        assert_eq!(log(&repo), ["c", "a"]);
        assert!(!dir.join("d.txt").exists());

        // The first commit, an id that is not on the branch, and a detached HEAD are refused.
        let first = log(&repo).len() - 1;
        let first_id = repo
            .revwalk()
            .map(|mut w| {
                w.push_head().unwrap();
                w.nth(first).unwrap().unwrap()
            })
            .unwrap();
        assert!(drop_commit(&repo, &first_id.to_string()).unwrap_err().contains("first commit"));
        assert!(drop_commit(&repo, &c.to_string()).is_err(), "c was re-created: the old id is no longer on the branch");
        assert!(drop_commit(&repo, "not-an-id").is_err());
        repo.set_head_detached(first_id).unwrap();
        assert!(drop_commit(&repo, &first_id.to_string()).unwrap_err().contains("Check out a branch"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_conflicting_drop_changes_nothing() {
        let (dir, repo) = new_repo("conflict");
        commit_file(&repo, &dir, "x.txt", "1", "a");
        let b = commit_file(&repo, &dir, "x.txt", "2", "b");
        commit_file(&repo, &dir, "x.txt", "3", "c"); // builds on b's change to the same line
        let d = commit_file(&repo, &dir, "other.txt", "fine", "d");

        let e = drop_commit(&repo, &b.to_string()).unwrap_err();
        assert!(e.contains("would conflict") && e.contains("x.txt") && e.contains("Nothing was changed"), "{e}");
        assert_eq!(repo.head().unwrap().target(), Some(d), "the branch did not move");
        assert_eq!(log(&repo), ["d", "c", "b", "a"]);
        assert_eq!(std::fs::read_to_string(dir.join("x.txt")).unwrap(), "3");
        assert!(crate::changes::status_of(&repo).unwrap().is_empty());
        assert_eq!(repo.state(), RepositoryState::Clean);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uncommitted_work_merges_and_pushed_commits() {
        let (dir, repo) = new_repo("rules");
        let a = commit_file(&repo, &dir, "a.txt", "one", "a");
        let branch = repo.head().unwrap().shorthand().unwrap().to_string();
        let b = commit_file(&repo, &dir, "b.txt", "bee", "b");

        // Uncommitted work blocks it, and nothing happens.
        std::fs::write(dir.join("a.txt"), "dirty").unwrap();
        assert!(drop_commit(&repo, &b.to_string()).unwrap_err().contains("uncommitted changes"));
        std::fs::write(dir.join("a.txt"), "one").unwrap();

        // A merge commit after the target makes the rewrite ambiguous; the merge itself can be dropped.
        let sig = Signature::now("D", "d@example.com").unwrap();
        let side_tree = repo.find_commit(a).unwrap().tree().unwrap();
        let side = repo.commit(None, &sig, &sig, "s", &side_tree, &[&repo.find_commit(a).unwrap()]).unwrap();
        let m = merge_commit(&repo, b, side);
        let c = commit_file(&repo, &dir, "c.txt", "sea", "c");
        let e = drop_commit(&repo, &b.to_string()).unwrap_err();
        assert!(e.contains("no merge commit after"), "{e}");
        assert!(drop_commit(&repo, &side.to_string()).is_err(), "a side branch commit is not on the main line");
        let info = drop_info(&repo, &m.to_string()).unwrap();
        assert!(info.is_merge && info.later_commits == 1);

        // Pushed commits are counted so the dialog can warn.
        repo.remote("origin", "https://example.com/r.git").unwrap();
        repo.reference(&format!("refs/remotes/origin/{branch}"), m, true, "test").unwrap();
        repo.find_branch(&branch, BranchType::Local).unwrap().set_upstream(Some(&format!("origin/{branch}"))).unwrap();
        let info = drop_info(&repo, &m.to_string()).unwrap();
        assert!(info.pushed);
        assert_eq!((info.later_commits, info.later_pushed), (1, 0), "c is not pushed yet");
        assert!(!drop_info(&repo, &c.to_string()).unwrap().pushed);

        // Dropping the merge replays c onto the merge's first parent.
        drop_commit(&repo, &m.to_string()).unwrap();
        assert_eq!(log(&repo), ["c", "b", "a"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
