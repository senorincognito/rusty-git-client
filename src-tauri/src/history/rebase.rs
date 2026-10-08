//! Interactive rebase: pick, reword, squash and drop commits, in a new order if wanted.

use std::collections::HashMap;

use git2::{Commit, Oid, Repository, RepositoryState, ResetType};
use serde::Serialize;

use super::{blocking, err, is_pushed, move_head_to};

/// Above this many commits the rebase editor would be unwieldy; pick a later base instead.
const MAX_REBASE_COMMITS: usize = 500;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RebaseCommit {
    pub id: String,
    pub short_id: String,
    /// Full message, to pre-fill the editor.
    pub message: String,
    pub author: String,
    /// Unix seconds.
    pub time: i64,
    /// Already on the upstream: changing it rewrites published history.
    pub pushed: bool,
    pub is_merge: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RebasePlan {
    /// HEAD when the plan was made; applying refuses if the branch moved since.
    pub head_id: String,
    /// The commits after the base on the current branch, newest first.
    pub commits: Vec<RebaseCommit>,
}

/// The commits an interactive rebase onto `base` covers: every commit after it on the current
/// branch's own (first-parent) line of history, newest first. `base` itself is not included, like
/// `git rebase -i <base>`.
fn rebase_plan(repo: &Repository, base_id: &str) -> Result<RebasePlan, String> {
    let base = Oid::from_str(base_id).map_err(err)?;
    let head = repo.head().and_then(|h| h.peel_to_commit()).map_err(|_| "There are no commits yet".to_string())?;
    let mut commits = Vec::new();
    let mut cursor = head.clone();
    while cursor.id() != base {
        if commits.len() == MAX_REBASE_COMMITS {
            return Err(format!(
                "More than {MAX_REBASE_COMMITS} commits follow this one; start the rebase from a later commit"
            ));
        }
        let id = cursor.id().to_string();
        commits.push(RebaseCommit {
            short_id: id[..7].to_string(),
            id,
            message: String::from_utf8_lossy(cursor.message_raw_bytes()).trim_end().to_string(),
            author: cursor.author().name().unwrap_or("").to_string(),
            time: cursor.time().seconds(),
            pushed: is_pushed(repo, cursor.id()),
            is_merge: cursor.parent_count() > 1,
        });
        cursor = cursor.parent(0).map_err(|_| {
            "Only commits on the current branch's own line of history can start an interactive rebase".to_string()
        })?;
    }
    if commits.is_empty() {
        return Err("There are no commits after this one on the current branch".into());
    }
    Ok(RebasePlan { head_id: head.id().to_string(), commits })
}

#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RebaseAction {
    /// Keep the commit as it is.
    Pick,
    /// Keep the commit but give it a new message.
    Reword,
    /// Meld the commit into the one before it (older), appending its message.
    Squash,
    /// Remove the commit and its changes from the branch.
    Drop,
}

#[derive(serde::Deserialize, Debug)]
pub struct RebaseStep {
    pub id: String,
    pub action: RebaseAction,
    /// The new message of a `Reword`.
    #[serde(default)]
    pub message: Option<String>,
}

/// One commit of the result: a commit of the plan plus the commits squashed into it.
struct RebaseGroup {
    /// The commit the group starts from (the oldest member) and the commits squashed into it, oldest first.
    head: Oid,
    squashed: Vec<Oid>,
    /// The message of the head: its own, or the reworded one.
    message: String,
    squashed_messages: Vec<String>,
    reworded: bool,
    dropped: bool,
    /// Position (oldest first, in the new order) of the newest commit of the group.
    last_pos: usize,
}

impl RebaseGroup {
    fn changed(&self) -> bool {
        self.reworded || self.dropped || !self.squashed.is_empty()
    }

    /// The head's message followed by the messages of the squashed commits, separated by blank lines.
    fn full_message(&self) -> String {
        let mut text = self.message.trim_end().to_string();
        for m in &self.squashed_messages {
            text.push_str("\n\n");
            text.push_str(m.trim());
        }
        text.push('\n');
        text
    }
}

/// Applies an interactive rebase made of `pick`, `reword`, `squash` and `drop` steps (commits without a step
/// are picked) keeping the planned order; see [`apply_rebase_ordered`]. Used by the tests.
#[cfg(test)]
fn apply_rebase(repo: &Repository, base_id: &str, head_id: &str, steps: &[RebaseStep]) -> Result<(), String> {
    apply_rebase_ordered(repo, base_id, head_id, steps, &[])
}

/// Applies an interactive rebase. `order` lists the ids of all the plan's commits newest first in the order
/// they should end up in (empty: unchanged); `steps` say what to do with each. A squashed commit is melded
/// into the commit before it in the new order (following a chain of squashes to the commit that starts it);
/// the result keeps that commit's author and parents, and its message is the older message followed by the
/// squashed ones. Every commit after the oldest change is rebuilt on top, then the branch moves; as long as
/// nothing is dropped or moved the files and the index stay as they are because the tip's content never changes.
///
/// A `drop` or a changed order changes content: from the first dropped or moved commit on, the commits are
/// replayed in memory with `cherrypick_commit` onto the rebuilt history (3-way merges). If one of them depends
/// on a commit that was dropped or moved behind it, the whole rebase is abandoned with nothing changed; otherwise
/// a hard reset moves the branch, index and files, so the working directory must be clean. Merge commits can't
/// be replayed that way, so they are refused from the first drop or move on.
///
/// Refuses if HEAD is no longer `head_id`, so a plan shown earlier can't be applied to a branch that moved meanwhile.
fn apply_rebase_ordered(
    repo: &Repository,
    base_id: &str,
    head_id: &str,
    steps: &[RebaseStep],
    order: &[String],
) -> Result<(), String> {
    if repo.state() != RepositoryState::Clean {
        return Err("Finish the merge, rebase or other operation in progress first".into());
    }
    let plan = rebase_plan(repo, base_id)?;
    if plan.head_id != head_id {
        return Err("The branch has changed since the rebase was opened. Close it and start again.".into());
    }
    let base = Oid::from_str(base_id).map_err(err)?;

    let mut chosen: HashMap<&str, &RebaseStep> = HashMap::new();
    for step in steps {
        if !plan.commits.iter().any(|c| c.id == step.id) {
            return Err(format!("Commit {} is not part of this rebase", &step.id[..7.min(step.id.len())]));
        }
        chosen.insert(step.id.as_str(), step);
    }

    // The commits oldest first, in the order they should end up in.
    let original: Vec<&RebaseCommit> = plan.commits.iter().rev().collect();
    let ordered: Vec<&RebaseCommit> = if order.is_empty() {
        original.clone()
    } else {
        let mut listed = Vec::new();
        for id in order.iter().rev() {
            let c = plan
                .commits
                .iter()
                .find(|c| &c.id == id)
                .ok_or_else(|| format!("Commit {} is not part of this rebase", &id[..7.min(id.len())]))?;
            listed.push(c);
        }
        let mut sorted: Vec<&str> = listed.iter().map(|c| c.id.as_str()).collect();
        sorted.sort_unstable();
        sorted.dedup();
        if listed.len() != plan.commits.len() || sorted.len() != listed.len() {
            return Err("The new order must list every commit of the rebase exactly once".into());
        }
        listed
    };
    // From this position on the commits differ from the original history, so their trees can't be reused.
    let first_moved = original.iter().zip(&ordered).position(|(o, n)| o.id != n.id);

    // Group every squashed commit with the commit it melds into.
    let mut groups: Vec<RebaseGroup> = Vec::new();
    for (pos, c) in ordered.iter().enumerate() {
        let oid = Oid::from_str(&c.id).map_err(err)?;
        let step = chosen.get(c.id.as_str());
        match step.map_or(RebaseAction::Pick, |s| s.action) {
            RebaseAction::Squash => {
                if c.is_merge {
                    return Err(format!("{} is a merge commit and can't be squashed", c.short_id));
                }
                let target = groups.last_mut().ok_or_else(|| {
                    format!(
                        "{} is the oldest commit of this rebase: there is nothing before it to squash into",
                        c.short_id
                    )
                })?;
                if target.dropped {
                    return Err(format!("{} can't be squashed into a commit that is dropped", c.short_id));
                }
                target.squashed.push(oid);
                target.squashed_messages.push(c.message.clone());
                target.last_pos = pos;
            }
            RebaseAction::Drop => groups.push(RebaseGroup {
                head: oid,
                squashed: Vec::new(),
                message: c.message.clone(),
                squashed_messages: Vec::new(),
                reworded: false,
                dropped: true,
                last_pos: pos,
            }),
            action => {
                let mut message = c.message.clone();
                let mut reworded = false;
                if action == RebaseAction::Reword {
                    let new = git2::message_prettify(step.and_then(|s| s.message.as_deref()).unwrap_or(""), None)
                        .map_err(err)?;
                    if new.trim().is_empty() {
                        return Err(format!("The message of {} is empty", c.short_id));
                    }
                    // Giving a commit its own message back changes nothing.
                    if new.trim() != c.message.trim() {
                        message = new;
                        reworded = true;
                    }
                }
                groups.push(RebaseGroup {
                    head: oid,
                    squashed: Vec::new(),
                    message,
                    squashed_messages: Vec::new(),
                    reworded,
                    dropped: false,
                    last_pos: pos,
                });
            }
        }
    }
    let moved = |g: &RebaseGroup| first_moved.is_some_and(|m| g.last_pos >= m);
    let first_changed = groups
        .iter()
        .position(|g| g.changed() || moved(g))
        .ok_or("Nothing to change: reword, squash, drop or move at least one commit")?;

    let will_replay = first_moved.is_some() || groups.iter().any(|g| g.dropped);
    if will_replay && !crate::changes::status_of(repo)?.is_empty() {
        return Err("The working directory has uncommitted changes. Commit or stash them first: dropping or moving \
                    commits resets the files to the new history."
            .into());
    }

    let mut rebuilt: HashMap<Oid, Oid> = HashMap::new();
    // What the next commit is built on: the rebuilt commit before it (the base for the first one).
    let mut tip = if first_changed == 0 { base } else { groups[first_changed - 1].head };
    // Once a commit is dropped or moved the later trees can no longer be reused: they are replayed instead.
    let mut replaying = false;
    for group in &groups[first_changed..] {
        if group.dropped {
            replaying = true;
            rebuilt.insert(group.head, tip); // whatever was built on it is built on what is below it
            continue;
        }
        replaying = replaying || moved(group);

        let head = repo.find_commit(group.head).map_err(err)?;
        let parents = if replaying {
            vec![repo.find_commit(tip).map_err(err)?]
        } else {
            head.parent_ids()
                .map(|p| repo.find_commit(*rebuilt.get(&p).unwrap_or(&p)).map_err(err))
                .collect::<Result<Vec<_>, _>>()?
        };

        let mut members = vec![group.head];
        members.extend(&group.squashed);
        let tree = if replaying {
            replay_group(repo, &members, &parents)?
        } else {
            // Each commit contains the ones before it: the group's content is that of its newest commit.
            let newest = repo.find_commit(*members.last().unwrap()).map_err(err)?;
            newest.tree().map_err(err)?
        };

        let parent_refs: Vec<&Commit> = parents.iter().collect();
        let (text, committer) = if group.changed() || replaying {
            // Like --amend: the person rebasing becomes the committer (falling back to the old one).
            let message = if group.changed() {
                group.full_message()
            } else {
                String::from_utf8_lossy(head.message_raw_bytes()).into_owned()
            };
            (message, repo.signature().unwrap_or_else(|_| head.committer().to_owned()))
        } else {
            (String::from_utf8_lossy(head.message_raw_bytes()).into_owned(), head.committer().to_owned())
        };
        let new = repo.commit(None, &head.author(), &committer, &text, &tree, &parent_refs).map_err(err)?;
        for m in members {
            rebuilt.insert(m, new);
        }
        tip = new;
    }

    if will_replay {
        let tip = repo.find_commit(tip).map_err(err)?;
        return repo.reset(tip.as_object(), ResetType::Hard, None).map_err(err);
    }
    let reworded = groups.iter().filter(|g| g.reworded).count();
    let squashed: usize = groups.iter().map(|g| g.squashed.len()).sum();
    let note = format!("interactive rebase: {reworded} reworded, {squashed} squashed");
    move_head_to(repo, tip, &note)
}

/// The tree of a group's commits (`members`, oldest first) replayed onto `parents[0]`, the rebuilt history
/// below them: each commit's own changes are 3-way merged in turn. A conflict abandons the whole rebase.
fn replay_group<'r>(repo: &'r Repository, members: &[Oid], parents: &[Commit<'r>]) -> Result<git2::Tree<'r>, String> {
    let mut base = parents.first().ok_or("The first commit of a branch can't be replayed")?.clone();
    let mut tree = base.tree().map_err(err)?;
    for (n, oid) in members.iter().enumerate() {
        let commit = repo.find_commit(*oid).map_err(err)?;
        if commit.parent_count() != 1 {
            return Err(format!(
                "{} is a merge commit and can't be replayed after a dropped or moved commit",
                &oid.to_string()[..7]
            ));
        }
        let mut merged = repo.cherrypick_commit(&commit, &base, 0, None).map_err(err)?;
        if merged.has_conflicts() {
            let files: Vec<String> = merged
                .conflicts()
                .map_err(err)?
                .flatten()
                .filter_map(|c| c.our.or(c.their).or(c.ancestor))
                .map(|e| String::from_utf8_lossy(&e.path).into_owned())
                .collect();
            return Err(format!(
                "The commit {} depends on a commit that was dropped or moved behind it ({}). Nothing was changed.",
                &oid.to_string()[..7],
                files.join(", ")
            ));
        }
        tree = repo.find_tree(merged.write_tree_to(repo).map_err(err)?).map_err(err)?;
        if n + 1 < members.len() {
            // The next member is merged on top of this result: give it a commit to stand on.
            let sig = commit.committer().to_owned();
            let step = repo.commit(None, &sig, &sig, "squash step", &tree, &[&base]).map_err(err)?;
            base = repo.find_commit(step).map_err(err)?;
        }
    }
    Ok(tree)
}

/// The commits an interactive rebase onto `id` would cover (newest first).
#[tauri::command]
pub async fn get_rebase_plan(path: String, id: String) -> Result<RebasePlan, String> {
    blocking(path, move |r| rebase_plan(r, &id)).await
}

/// Applies an interactive rebase of pick / reword / squash / drop steps in a given order (see [`apply_rebase_ordered`]).
#[tauri::command]
pub async fn apply_rebase_cmd(
    path: String,
    base_id: String,
    head_id: String,
    steps: Vec<RebaseStep>,
    order: Vec<String>,
) -> Result<(), String> {
    crate::undo::recorded(&path.clone(), "Interactive rebase", crate::undo::Kind::Switch, || {
        blocking(path, move |r| apply_rebase_ordered(r, &base_id, &head_id, &steps, &order))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{commit_file, log, merge_commit, new_repo};
    use git2::Signature;

    #[test]
    fn interactive_rebase_rewords_several_commits() {
        let (dir, repo) = new_repo("reword");
        let base = commit_file(&repo, &dir, "a.txt", "1", "base");
        let b = commit_file(&repo, &dir, "a.txt", "2", "b");
        let c = commit_file(&repo, &dir, "a.txt", "3", "c");
        let d = commit_file(&repo, &dir, "a.txt", "4", "d");
        repo.config().unwrap().set_str("user.name", "Editor").unwrap();
        repo.config().unwrap().set_str("user.email", "e@example.com").unwrap();

        // The plan covers the commits after the base, newest first; the base itself is not part of it.
        let plan = rebase_plan(&repo, &base.to_string()).unwrap();
        let ids: Vec<String> = plan.commits.iter().map(|c| c.id.clone()).collect();
        assert_eq!(ids, [d.to_string(), c.to_string(), b.to_string()]);
        assert!(rebase_plan(&repo, &d.to_string()).unwrap_err().contains("no commits after"));

        let edit =
            |id: Oid, m: &str| RebaseStep { id: id.to_string(), action: RebaseAction::Reword, message: Some(m.into()) };
        let head = plan.head_id.clone();
        let bs = base.to_string();
        // Refusals: nothing changed, an empty message, a commit outside the plan, a stale plan.
        assert!(apply_rebase(&repo, &bs, &head, &[edit(c, "c")]).unwrap_err().contains("Nothing to change"));
        assert!(apply_rebase(&repo, &bs, &head, &[edit(c, "  ")]).unwrap_err().contains("empty"));
        assert!(apply_rebase(&repo, &bs, &head, &[edit(base, "x")]).unwrap_err().contains("not part"));
        assert!(apply_rebase(&repo, &bs, &b.to_string(), &[edit(c, "x")]).unwrap_err().contains("changed since"));

        // Reword b and d (c in between is rebuilt with its old message).
        apply_rebase(&repo, &bs, &head, &[edit(d, "D!"), edit(c, "c"), edit(b, "B!\n\nbody")]).unwrap();
        assert_eq!(log(&repo), ["D!", "c", "B!", "base"]);
        let new_head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(new_head.tree_id(), repo.find_commit(d).unwrap().tree_id(), "same content");
        assert_eq!(new_head.committer().name(), Ok("Editor"));
        let new_c = new_head.parent(0).unwrap();
        assert_eq!(new_c.committer().name(), Ok("D"), "an untouched commit keeps its committer");
        assert_eq!(new_c.parent(0).unwrap().message(), Ok("B!\n\nbody\n"));
        assert_eq!(new_c.parent(0).unwrap().parent_id(0).unwrap(), base, "the base is untouched");
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "4");

        // A commit beside the current line of history can't be a base.
        repo.branch("side", &repo.find_commit(b).unwrap(), false).unwrap();
        let other = repo.find_commit(base).unwrap();
        let sig = Signature::now("D", "d@example.com").unwrap();
        let side = repo
            .commit(
                Some("refs/heads/side"),
                &sig,
                &sig,
                "side",
                &other.tree().unwrap(),
                &[&repo.find_commit(b).unwrap()],
            )
            .unwrap();
        assert!(rebase_plan(&repo, &side.to_string()).unwrap_err().contains("own line"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn interactive_rebase_squashes_into_the_commit_before() {
        let (dir, repo) = new_repo("squash");
        let base = commit_file(&repo, &dir, "a.txt", "1", "base");
        let b = commit_file(&repo, &dir, "a.txt", "2", "b\n\nabout b");
        let c = commit_file(&repo, &dir, "a.txt", "3", "c");
        commit_file(&repo, &dir, "a.txt", "4", "d");
        let e = commit_file(&repo, &dir, "a.txt", "5", "e");
        repo.config().unwrap().set_str("user.name", "Editor").unwrap();
        repo.config().unwrap().set_str("user.email", "e@example.com").unwrap();
        let bs = base.to_string();
        let plan = rebase_plan(&repo, &bs).unwrap();
        let head = plan.head_id.clone();
        let step = |id: Oid, action: RebaseAction, m: Option<&str>| RebaseStep {
            id: id.to_string(),
            action,
            message: m.map(str::to_string),
        };

        // Refusals: the oldest commit has nothing before it; nothing changes when only picks are given.
        let e1 = apply_rebase(&repo, &bs, &head, &[step(b, RebaseAction::Squash, None)]).unwrap_err();
        assert!(e1.contains("oldest commit"), "{e1}");
        let e2 = apply_rebase(&repo, &bs, &head, &[step(c, RebaseAction::Pick, None)]).unwrap_err();
        assert!(e2.contains("Nothing to change"), "{e2}");

        // Squash c into b, and e into d (which is itself picked): b+c, d+e remain.
        apply_rebase(&repo, &bs, &head, &[step(c, RebaseAction::Squash, None), step(e, RebaseAction::Squash, None)])
            .unwrap();
        assert_eq!(log(&repo), ["d", "b", "base"]);
        let tip = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(tip.message(), Ok("d\n\ne\n"), "the squashed message is appended");
        assert_eq!(tip.tree_id(), repo.find_commit(e).unwrap().tree_id(), "the content of the newest commit");
        let first = tip.parent(0).unwrap();
        assert_eq!(first.message(), Ok("b\n\nabout b\n\nc\n"));
        assert_eq!(first.tree_id(), repo.find_commit(c).unwrap().tree_id());
        assert_eq!(first.parent_id(0).unwrap(), base);
        assert_eq!(first.author().name(), Ok("D"), "the older commit's author stays");
        assert_eq!(first.committer().name(), Ok("Editor"));
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "5");

        // A chain: squash two in a row into a reworded commit, with a later commit rebuilt on top.
        let (dir2, repo2) = new_repo("squash-chain");
        let base = commit_file(&repo2, &dir2, "a.txt", "1", "base");
        let b = commit_file(&repo2, &dir2, "a.txt", "2", "b");
        let c = commit_file(&repo2, &dir2, "a.txt", "3", "c");
        let d = commit_file(&repo2, &dir2, "a.txt", "4", "d");
        let e = commit_file(&repo2, &dir2, "a.txt", "5", "e");
        let bs = base.to_string();
        let head = rebase_plan(&repo2, &bs).unwrap().head_id;
        apply_rebase(
            &repo2,
            &bs,
            &head,
            &[
                step(b, RebaseAction::Reword, Some("B")),
                step(c, RebaseAction::Squash, None),
                step(d, RebaseAction::Squash, None),
            ],
        )
        .unwrap();
        assert_eq!(log(&repo2), ["e", "B", "base"]);
        let tip = repo2.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(tip.committer().name(), Ok("D"), "an untouched commit keeps its committer");
        assert_eq!(tip.tree_id(), repo2.find_commit(e).unwrap().tree_id());
        assert_eq!(tip.parent(0).unwrap().message(), Ok("B\n\nc\n\nd\n"));

        // A merge commit can't be squashed (its second parent would be lost), but squashing into one is fine.
        let (dir3, repo3) = new_repo("squash-merge");
        let base = commit_file(&repo3, &dir3, "a.txt", "1", "base");
        let side = {
            let sig = Signature::now("D", "d@example.com").unwrap();
            repo3
                .commit(
                    None,
                    &sig,
                    &sig,
                    "side",
                    &repo3.find_commit(base).unwrap().tree().unwrap(),
                    &[&repo3.find_commit(base).unwrap()],
                )
                .unwrap()
        };
        let m = merge_commit(&repo3, base, side);
        let after = commit_file(&repo3, &dir3, "a.txt", "2", "after");
        let bs = base.to_string();
        let head = rebase_plan(&repo3, &bs).unwrap().head_id;
        let e = apply_rebase(&repo3, &bs, &head, &[step(m, RebaseAction::Squash, None)]).unwrap_err();
        assert!(e.contains("merge commit"), "{e}");
        apply_rebase(&repo3, &bs, &head, &[step(after, RebaseAction::Squash, None)]).unwrap();
        let tip = repo3.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(tip.parent_count(), 2, "the merge keeps both parents");
        assert_eq!(tip.message(), Ok("merge\n\nafter\n"));
        assert_eq!(tip.tree_id(), repo3.find_commit(after).unwrap().tree_id());

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir2);
        let _ = std::fs::remove_dir_all(&dir3);
    }

    #[test]
    fn interactive_rebase_drops_commits() {
        let (dir, repo) = new_repo("rebase-drop");
        let base = commit_file(&repo, &dir, "base.txt", "0", "base");
        let a = commit_file(&repo, &dir, "a.txt", "a", "add a");
        let b = commit_file(&repo, &dir, "b.txt", "b", "add b");
        let c = commit_file(&repo, &dir, "c.txt", "c", "add c");
        let d = commit_file(&repo, &dir, "a.txt", "a2", "change a");
        repo.config().unwrap().set_str("user.name", "Editor").unwrap();
        repo.config().unwrap().set_str("user.email", "e@example.com").unwrap();
        let bs = base.to_string();
        let head = rebase_plan(&repo, &bs).unwrap().head_id;
        let step = |id: Oid, action: RebaseAction, m: Option<&str>| RebaseStep {
            id: id.to_string(),
            action,
            message: m.map(str::to_string),
        };

        // Dropping a commit that a later one builds on conflicts: nothing changes at all.
        let e = apply_rebase(&repo, &bs, &head, &[step(a, RebaseAction::Drop, None)]).unwrap_err();
        assert!(e.contains("depends on") && e.contains("a.txt"), "{e}");
        assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), d);
        assert_eq!(log(&repo), ["change a", "add c", "add b", "add a", "base"]);

        // Squashing into a dropped commit is refused; so is a dirty working directory.
        let e =
            apply_rebase(&repo, &bs, &head, &[step(b, RebaseAction::Drop, None), step(c, RebaseAction::Squash, None)])
                .unwrap_err();
        assert!(e.contains("dropped"), "{e}");
        std::fs::write(dir.join("dirty.txt"), "x").unwrap();
        let e = apply_rebase(&repo, &bs, &head, &[step(b, RebaseAction::Drop, None)]).unwrap_err();
        assert!(e.contains("uncommitted changes"), "{e}");
        std::fs::remove_file(dir.join("dirty.txt")).unwrap();

        // Drop b (independent of the rest) and reword c: later commits are replayed on top, files follow.
        apply_rebase(
            &repo,
            &bs,
            &head,
            &[step(b, RebaseAction::Drop, None), step(c, RebaseAction::Reword, Some("C!"))],
        )
        .unwrap();
        assert_eq!(log(&repo), ["change a", "C!", "add a", "base"]);
        assert!(!dir.join("b.txt").exists(), "the dropped commit's file is gone");
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "a2");
        assert_eq!(std::fs::read_to_string(dir.join("c.txt")).unwrap(), "c");
        let tip = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(tip.author().name(), Ok("D"), "the author is kept");
        assert!(crate::changes::status_of(&repo).unwrap().is_empty(), "the working directory matches the new tip");
        let rewritten_a = tip.parent(0).unwrap().parent(0).unwrap();
        assert_eq!(rewritten_a.parent_id(0).unwrap(), base, "commits before the first change are untouched");
        assert_eq!(rewritten_a.id(), a);

        // Drop everything above the base, including the tip.
        let bs2 = base.to_string();
        let head2 = rebase_plan(&repo, &bs2).unwrap();
        let steps: Vec<RebaseStep> = head2
            .commits
            .iter()
            .map(|c| RebaseStep { id: c.id.clone(), action: RebaseAction::Drop, message: None })
            .collect();
        apply_rebase(&repo, &bs2, &head2.head_id, &steps).unwrap();
        assert_eq!(log(&repo), ["base"]);
        assert!(!dir.join("a.txt").exists() && !dir.join("c.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dropping_in_a_rebase_refuses_merges_after_it_but_not_before() {
        let (dir, repo) = new_repo("rebase-drop-merge");
        let base = commit_file(&repo, &dir, "base.txt", "0", "base");
        let side = {
            let sig = Signature::now("D", "d@example.com").unwrap();
            repo.commit(
                None,
                &sig,
                &sig,
                "side",
                &repo.find_commit(base).unwrap().tree().unwrap(),
                &[&repo.find_commit(base).unwrap()],
            )
            .unwrap()
        };
        let x = commit_file(&repo, &dir, "x.txt", "x", "x");
        let m = merge_commit(&repo, x, side);
        let y = commit_file(&repo, &dir, "y.txt", "y", "y");
        let z = commit_file(&repo, &dir, "z.txt", "z", "z");
        let bs = base.to_string();
        let head = rebase_plan(&repo, &bs).unwrap().head_id;
        let step = |id: Oid, action: RebaseAction| RebaseStep { id: id.to_string(), action, message: None };

        // x -> m -> y -> z: dropping x means replaying the merge m, which isn't supported.
        let e = apply_rebase(&repo, &bs, &head, &[step(x, RebaseAction::Drop)]).unwrap_err();
        assert!(e.contains("merge commit"), "{e}");
        // Dropping y (after the merge) is fine: the merge is before the first change and stays as it is.
        apply_rebase(&repo, &bs, &head, &[step(y, RebaseAction::Drop)]).unwrap();
        assert_eq!(log(&repo).first().map(String::as_str), Some("z"));
        let tip = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(tip.parent_id(0).unwrap(), m, "the merge commit is untouched");
        assert!(!dir.join("y.txt").exists());
        assert!(dir.join("z.txt").exists());
        let _ = z;

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn interactive_rebase_reorders_commits() {
        let (dir, repo) = new_repo("rebase-move");
        let base = commit_file(&repo, &dir, "base.txt", "0", "base");
        let a = commit_file(&repo, &dir, "a.txt", "a", "add a");
        let b = commit_file(&repo, &dir, "b.txt", "b", "add b");
        let c = commit_file(&repo, &dir, "c.txt", "c", "add c");
        let d = commit_file(&repo, &dir, "a.txt", "a2", "change a");
        repo.config().unwrap().set_str("user.name", "Editor").unwrap();
        repo.config().unwrap().set_str("user.email", "e@example.com").unwrap();
        let bs = base.to_string();
        let head = rebase_plan(&repo, &bs).unwrap().head_id;
        let ids = |list: &[Oid]| list.iter().map(|o| o.to_string()).collect::<Vec<_>>();

        // Malformed orders and a pure no-op are refused.
        let e = apply_rebase_ordered(&repo, &bs, &head, &[], &ids(&[d, c, b])).unwrap_err();
        assert!(e.contains("every commit"), "{e}");
        let e = apply_rebase_ordered(&repo, &bs, &head, &[], &ids(&[d, c, b, a])).unwrap_err();
        assert!(e.contains("Nothing to change"), "{e}");

        // "change a" can't move below "add a": it would lose what it changes. Nothing happens.
        let e = apply_rebase_ordered(&repo, &bs, &head, &[], &ids(&[c, b, a, d])).unwrap_err();
        assert!(e.contains("depends on") && e.contains("a.txt"), "{e}");
        assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), d);

        // Swap "add b" and "add c" (newest first: d, b, c, a): files stay, history follows the new order.
        std::fs::write(dir.join("dirty.txt"), "x").unwrap();
        let e = apply_rebase_ordered(&repo, &bs, &head, &[], &ids(&[d, b, c, a])).unwrap_err();
        assert!(e.contains("uncommitted changes"), "{e}");
        std::fs::remove_file(dir.join("dirty.txt")).unwrap();
        apply_rebase_ordered(&repo, &bs, &head, &[], &ids(&[d, b, c, a])).unwrap();
        assert_eq!(log(&repo), ["change a", "add b", "add c", "add a", "base"]);
        let tip = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(tip.author().name(), Ok("D"));
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "a2");
        assert!(dir.join("b.txt").exists() && dir.join("c.txt").exists());
        assert!(crate::changes::status_of(&repo).unwrap().is_empty());
        let first = tip.parent(0).unwrap().parent(0).unwrap().parent(0).unwrap();
        assert_eq!(first.id(), a, "commits before the first move are untouched");

        // Move + squash use the new order: put "add c" below "add b" again and squash "add b" into it.
        let plan = rebase_plan(&repo, &bs).unwrap();
        let by = |msg: &str| plan.commits.iter().find(|c| c.message == msg).unwrap().id.clone();
        let (b2, c2, d2) = (by("add b"), by("add c"), by("change a"));
        let order = vec![d2.clone(), c2.clone(), b2.clone(), by("add a")];
        let steps = [RebaseStep { id: c2.clone(), action: RebaseAction::Squash, message: None }];
        apply_rebase_ordered(&repo, &bs, &plan.head_id, &steps, &order).unwrap();
        // "add b" ends up before "add c"; "add c" melds into it.
        assert_eq!(log(&repo), ["change a", "add b", "add a", "base"]);
        assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().parent(0).unwrap().message(), Ok("add b\n\nadd c\n"));
        assert!(dir.join("c.txt").exists() && dir.join("b.txt").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
