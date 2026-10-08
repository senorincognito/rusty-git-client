//! Rename a commit: rebuild it and every later commit on the branch with a new message.

use std::collections::HashMap;

use git2::{Oid, Repository, RepositoryState};
use serde::Serialize;

use super::{blocking, err, is_pushed, on_current_branch, rebuild_with_messages, rewrite_plan};

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RenameInfo {
    /// Full current message, to pre-fill the editor.
    pub message: String,
    /// The commit is already on the upstream: renaming it needs a force push.
    pub pushed: bool,
    /// How many later commits will be rewritten along with it.
    pub later_commits: usize,
}

fn rename_info(repo: &Repository, id: &str) -> Result<RenameInfo, String> {
    let oid = Oid::from_str(id).map_err(err)?;
    let commit = repo.find_commit(oid).map_err(err)?;
    if !on_current_branch(repo, oid) {
        return Err("Only commits on the current branch can be renamed".into());
    }
    let plan = rewrite_plan(repo, &commit)?;
    Ok(RenameInfo {
        message: String::from_utf8_lossy(commit.message_raw_bytes()).trim_end().to_string(),
        pushed: is_pushed(repo, oid),
        later_commits: plan.len().saturating_sub(1),
    })
}

/// Changes the message of `id` and rebuilds every later commit on the current branch on top
/// of it (same trees, authors and timestamps), then moves the branch to the new tip, like a
/// `reword` in an interactive rebase. Files and index are untouched because no tree changes.
/// Returns the id of the renamed commit.
fn rename_commit(repo: &Repository, id: &str, message: &str) -> Result<Oid, String> {
    let message = git2::message_prettify(message, None).map_err(err)?;
    if message.trim().is_empty() {
        return Err("Commit message is empty".into());
    }
    if repo.state() != RepositoryState::Clean {
        return Err("Finish the merge, rebase or other operation in progress first".into());
    }
    let target_oid = Oid::from_str(id).map_err(err)?;
    let target = repo.find_commit(target_oid).map_err(err)?;
    if !on_current_branch(repo, target_oid) {
        return Err("Only commits on the current branch can be renamed".into());
    }
    if String::from_utf8_lossy(target.message_raw_bytes()).trim() == message.trim() {
        return Err("The message is unchanged".into());
    }

    let edits = HashMap::from([(target_oid, message)]);
    let rebuilt = rebuild_with_messages(repo, &target, &edits, &format!("rename commit {}", &id[..7.min(id.len())]))?;
    Ok(rebuilt[&target_oid])
}

/// What the rename dialog needs: current message and the consequences of renaming.
#[tauri::command]
pub async fn get_rename_info(path: String, id: String) -> Result<RenameInfo, String> {
    blocking(path, move |r| rename_info(r, &id)).await
}

/// Renames a commit on the current branch. Returns the new id of that commit.
#[tauri::command]
pub async fn rename_commit_message(path: String, id: String, message: String) -> Result<String, String> {
    crate::undo::recorded(&path.clone(), "Rename commit", crate::undo::Kind::Switch, || {
        blocking(path, move |r| rename_commit(r, &id, &message).map(|o| o.to_string()))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{make, msg};
    use git2::BranchType;

    #[test]
    fn rename_rewrites_descendants_only() {
        let dir = std::env::temp_dir().join(format!("gc-rename-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Renamer").unwrap();
        cfg.set_str("user.email", "r@example.com").unwrap();
        cfg.set_str("core.autocrlf", "false").unwrap(); // the result must not depend on the machine's git config

        //   a - b ----- m - c   (main)
        //    \         /
        //     s -------      (side, merged into m)
        let a = make(&repo, "Alice", "a", &[], Some("refs/heads/main"));
        repo.set_head("refs/heads/main").unwrap();
        let b = make(&repo, "Bob", "b", &[a], Some("refs/heads/main"));
        let s = make(&repo, "Sue", "s", &[a], Some("refs/heads/side"));
        let m = make(&repo, "Mia", "m", &[b, s], Some("refs/heads/main"));
        let c = make(&repo, "Cy", "c", &[m], Some("refs/heads/main"));
        let other = make(&repo, "Oz", "o", &[a], Some("refs/heads/other"));

        // Info: how much gets rewritten, and which commits are eligible.
        assert_eq!(rename_info(&repo, &c.to_string()).unwrap().later_commits, 0);
        assert_eq!(rename_info(&repo, &b.to_string()).unwrap().later_commits, 2);
        assert_eq!(rename_info(&repo, &a.to_string()).unwrap().later_commits, 4);
        assert_eq!(rename_info(&repo, &b.to_string()).unwrap().message, "b");
        assert!(rename_info(&repo, &other.to_string()).unwrap_err().contains("current branch"));
        assert!(rename_commit(&repo, &other.to_string(), "x").unwrap_err().contains("current branch"));

        // Bad input.
        assert!(rename_commit(&repo, &b.to_string(), "  ").is_err());
        assert!(rename_commit(&repo, &b.to_string(), "b").unwrap_err().contains("unchanged"));

        // Rename a middle commit: b, m, c are rebuilt; s and the side branch are untouched.
        let new_b = rename_commit(&repo, &b.to_string(), "b, renamed").unwrap();
        assert_eq!(msg(&repo, new_b), "b, renamed");
        let new_head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_ne!(new_head.id(), c);
        assert_eq!(msg(&repo, new_head.id()), "c");
        assert_eq!(new_head.author().name(), Ok("Cy"));
        let new_m = new_head.parent(0).unwrap();
        assert_eq!(msg(&repo, new_m.id()), "m");
        assert_eq!(new_m.parent_id(0).unwrap(), new_b); // first parent follows the rename
        assert_eq!(new_m.parent_id(1).unwrap(), s); // merged-in branch is untouched
        let renamed = repo.find_commit(new_b).unwrap();
        assert_eq!(renamed.author().name(), Ok("Bob")); // author kept
        assert_eq!(renamed.committer().name(), Ok("Renamer")); // like --amend
        assert_eq!(repo.find_branch("side", BranchType::Local).unwrap().get().target(), Some(s));
        assert_eq!(repo.head().unwrap().shorthand(), Ok("main")); // still on the branch

        // Rename the tip itself, then the root (rewrites everything, side keeps the old history).
        let tip = repo.head().unwrap().peel_to_commit().unwrap().id();
        rename_commit(&repo, &tip.to_string(), "c, renamed").unwrap();
        assert_eq!(msg(&repo, repo.head().unwrap().peel_to_commit().unwrap().id()), "c, renamed");
        rename_commit(&repo, &a.to_string(), "a, renamed").unwrap();
        let mut walk = repo.revwalk().unwrap();
        walk.push_head().unwrap();
        let messages: Vec<_> = walk.map(|o| msg(&repo, o.unwrap())).collect();
        assert!(messages.contains(&"a, renamed".to_string()));
        assert!(!messages.contains(&"a".to_string()));
        assert_eq!(repo.find_branch("side", BranchType::Local).unwrap().get().target(), Some(s));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
