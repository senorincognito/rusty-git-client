//! The undo journal: every action of the app that changes the repository is recorded with a snapshot of the
//! relevant state before and after it, and Undo / Redo put that state back.
//!
//! A snapshot always holds HEAD, the local branches and the stash list. Depending on the action's [`Kind`] it also
//! holds the index (as a tree) and the whole working directory (also as a tree: tracked edits and untracked files).
//! An entry can only be undone while the repository is still exactly in its "after" state; if something else
//! changed it in between (the terminal, another tool), nothing is touched and the history is cleared instead of
//! guessing. The journal lives in memory, per repository, for the length of the session.
//!
//! Not recorded, because they can't be taken back locally: fetch, push, and everything that edits remotes.

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use git2::build::CheckoutBuilder;
use git2::{BranchType, Delta, IndexAddOption, Oid, Repository};
use serde::Serialize;

/// How many steps are remembered per repository.
const LIMIT: usize = 50;

/// What an action changes, which decides what is captured and how it is put back.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Only refs (branches, HEAD, stashes) move; the files and the index are left alone. A commit is the example:
    /// undoing it leaves the changes staged, like `git reset --soft`.
    Keep,
    /// Refs move and so do the files, but nothing uncommitted is involved (merge, fast-forward, checkout, rebase...).
    /// Undo checks out the old tree safely: it stops instead of overwriting changes made since.
    Switch,
    /// The index changes too (staging, mixed reset). It is captured as a tree and put back.
    Index,
    /// Uncommitted work is involved (discard, stash, hard reset): the index and the whole working directory are
    /// captured and put back.
    Full,
}

impl Kind {
    fn index(self) -> bool {
        matches!(self, Kind::Index | Kind::Full)
    }
    fn workdir(self) -> bool {
        self == Kind::Full
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum Head {
    Branch(String),
    Detached(Oid),
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct Snapshot {
    head: Head,
    /// Local branch name to tip.
    branches: BTreeMap<String, Oid>,
    /// The stash list, newest first: commit id and the list's message.
    stashes: Vec<(Oid, String)>,
    index: Option<Oid>,
    workdir: Option<Oid>,
}

#[derive(Clone, Debug)]
struct Entry {
    label: String,
    kind: Kind,
    before: Snapshot,
    after: Snapshot,
}

#[derive(Default)]
struct Journal {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
}

static JOURNALS: LazyLock<Mutex<HashMap<String, Journal>>> = LazyLock::new(Default::default);

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

/// A tree holding the index as it is on disk; None if it has conflicts.
fn index_tree(repo: &Repository) -> Option<Oid> {
    let mut index = repo.index().ok()?;
    index.read(true).ok()?;
    index.write_tree().ok()
}

/// A tree holding everything in the working directory: tracked files as they are now (deleted ones left out) and
/// untracked ones, but not ignored files. Built in memory; the repository's index file is not touched.
fn workdir_tree(repo: &Repository) -> Option<Oid> {
    let mut index = repo.index().ok()?;
    index.read(true).ok()?;
    index.update_all(["*"], None).ok()?;
    index.add_all(["*"], IndexAddOption::DEFAULT, None).ok()?;
    let tree = index.write_tree().ok();
    let _ = index.read(true); // forget the in-memory changes
    tree
}

fn snapshot(repo: &mut Repository, kind: Kind) -> Result<Snapshot, String> {
    let head = {
        let head_ref = repo.find_reference("HEAD").map_err(err)?;
        match head_ref.symbolic_target().ok().flatten() {
            Some(target) => Head::Branch(target.strip_prefix("refs/heads/").unwrap_or(target).to_string()),
            None => Head::Detached(head_ref.target().ok_or("HEAD points nowhere")?),
        }
    };
    let mut branches = BTreeMap::new();
    for item in repo.branches(Some(BranchType::Local)).map_err(err)? {
        let (branch, _) = item.map_err(err)?;
        if let (Ok(Some(name)), Some(oid)) = (branch.name(), branch.get().target()) {
            branches.insert(name.to_string(), oid);
        }
    }
    let stashes = crate::sidebar::stash::list_stashes(repo)?
        .into_iter()
        .filter_map(|s| Oid::from_str(&s.id).ok().map(|o| (o, s.message)))
        .collect();
    let index = if kind.index() { Some(index_tree(repo).ok_or("The index has conflicts")?) } else { None };
    let workdir =
        if kind.workdir() { Some(workdir_tree(repo).ok_or("The working directory can't be captured")?) } else { None };
    Ok(Snapshot { head, branches, stashes, index, workdir })
}

/// Opens the repository and returns it with the key its journal is stored under.
fn open(path: &str) -> Result<(Repository, String), String> {
    let repo = Repository::discover(path).map_err(err)?;
    let key = repo.path().to_string_lossy().into_owned();
    Ok((repo, key))
}

/// The state before an action, if it could be captured (an action never fails because it couldn't be recorded).
fn begin(path: &str, kind: Kind) -> Option<Snapshot> {
    let (mut repo, _) = open(path).ok()?;
    snapshot(&mut repo, kind).ok()
}

/// Records the finished action unless it changed nothing. A new action ends the redo history.
fn finish(path: &str, label: String, kind: Kind, before: Option<Snapshot>) {
    let Some(before) = before else { return };
    let Ok((mut repo, key)) = open(path) else { return };
    let Ok(after) = snapshot(&mut repo, kind) else { return };
    if before == after {
        return;
    }
    let mut journals = JOURNALS.lock().unwrap();
    let journal = journals.entry(key).or_default();
    journal.redo.clear();
    journal.undo.push(Entry { label, kind, before, after });
    if journal.undo.len() > LIMIT {
        journal.undo.remove(0);
    }
}

/// Runs `op` (an action of the app, started lazily by this call) and records it for Undo when it succeeds.
pub async fn recorded<T, Fut>(
    path: &str,
    label: impl Into<String>,
    kind: Kind,
    op: impl FnOnce() -> Fut,
) -> Result<T, String>
where
    Fut: Future<Output = Result<T, String>>,
{
    let owned = path.to_string();
    let before = tauri::async_runtime::spawn_blocking({
        let owned = owned.clone();
        move || begin(&owned, kind)
    })
    .await
    .ok()
    .flatten();
    let result = op().await;
    if result.is_ok() && before.is_some() {
        let label = label.into();
        let _ = tauri::async_runtime::spawn_blocking(move || finish(&owned, label, kind, before)).await;
    }
    result
}

/// "file.txt", or "3 files" for several: for the labels of staging actions.
pub fn describe_paths(paths: &[String]) -> String {
    match paths {
        [one] => one.rsplit('/').next().unwrap_or(one).to_string(),
        many => format!("{} files", many.len()),
    }
}

/// A branch or commit as a person would name it: "feature" for `refs/heads/feature`, "origin/x" for a remote branch, the
/// first seven characters of a commit id.
pub fn short_ref(target: &str) -> String {
    if let Some(name) = target.strip_prefix("refs/heads/").or_else(|| target.strip_prefix("refs/remotes/")) {
        name.to_string()
    } else {
        target.chars().take(7).collect()
    }
}

enum Failure {
    /// The repository is no longer in the state the entry expects.
    Diverged,
    Other(String),
}

impl From<String> for Failure {
    fn from(e: String) -> Self {
        Failure::Other(e)
    }
}

/// Whether the repository is, as far as the entry's kind captures, in the `expected` state.
fn matches(current: &Snapshot, expected: &Snapshot) -> bool {
    current == expected
}

/// Puts the working directory and the index back to the captured trees: files that were changed or deleted since come
/// back, files that were added since are removed.
fn restore_workdir(repo: &Repository, workdir: Oid, index: Oid) -> Result<(), String> {
    let current = workdir_tree(repo).ok_or("The working directory can't be captured")?;
    let wanted = repo.find_tree(workdir).map_err(err)?;
    let now = repo.find_tree(current).map_err(err)?;
    let root = repo.workdir().ok_or("The repository has no working directory")?.to_path_buf();

    let diff = repo.diff_tree_to_tree(Some(&wanted), Some(&now), None).map_err(err)?;
    let mut restore: Vec<String> = Vec::new();
    for delta in diff.deltas() {
        match delta.status() {
            // Only in the current state: it did not exist back then.
            Delta::Added => {
                if let Some(p) = delta.new_file().path() {
                    let file = root.join(p);
                    let _ = std::fs::remove_file(&file);
                    // An emptied folder that was created along with it goes too.
                    let mut dir = file.parent().map(Path::to_path_buf);
                    while let Some(d) = dir.filter(|d| *d != root) {
                        if std::fs::remove_dir(&d).is_err() {
                            break;
                        }
                        dir = d.parent().map(Path::to_path_buf);
                    }
                }
            }
            _ => {
                if let Some(p) = delta.old_file().path() {
                    restore.push(p.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    if !restore.is_empty() {
        let mut opts = CheckoutBuilder::new();
        opts.force();
        for p in &restore {
            opts.path(p);
        }
        repo.checkout_tree(wanted.as_object(), Some(&mut opts)).map_err(err)?;
    }

    let mut idx = repo.index().map_err(err)?;
    idx.read_tree(&repo.find_tree(index).map_err(err)?).map_err(err)?;
    idx.write().map_err(err)
}

/// Checks out the tree of the snapshot's HEAD commit over the current one, refusing to overwrite changes made since.
fn switch_files(repo: &Repository, target: &Snapshot) -> Result<(), String> {
    let tip = match &target.head {
        Head::Branch(name) => target.branches.get(name).copied(),
        Head::Detached(oid) => Some(*oid),
    };
    let Some(tip) = tip else { return Ok(()) }; // an unborn branch has no files
    let tree = repo.find_commit(tip).map_err(err)?.tree().map_err(err)?;
    let mut opts = CheckoutBuilder::new();
    opts.safe();
    repo.checkout_tree(tree.as_object(), Some(&mut opts)).map_err(|e| {
        if e.code() == git2::ErrorCode::Conflict {
            "Changes you made since would be overwritten. Commit, stash or discard them first.".to_string()
        } else {
            err(e)
        }
    })
}

/// Makes the branches and HEAD what the snapshot says.
fn restore_refs(repo: &Repository, target: &Snapshot) -> Result<(), String> {
    for (name, oid) in &target.branches {
        repo.reference(&format!("refs/heads/{name}"), *oid, true, "undo").map_err(err)?;
    }
    match &target.head {
        Head::Branch(name) => repo.set_head(&format!("refs/heads/{name}")).map_err(err)?,
        Head::Detached(oid) => repo.set_head_detached(*oid).map_err(err)?,
    }
    for item in repo.branches(Some(BranchType::Local)).map_err(err)? {
        let (branch, _) = item.map_err(err)?;
        if let Ok(Some(name)) = branch.name() {
            if !target.branches.contains_key(name) {
                branch.into_reference().delete().map_err(err)?;
            }
        }
    }
    Ok(())
}

/// Makes the stash list what the snapshot says: all stashes go, then the wanted ones are stored again, oldest first
/// (the commits survive a drop, so `git stash store` can bring them back with the same message).
fn restore_stashes(path: &str, repo: &mut Repository, target: &Snapshot) -> Result<(), String> {
    let current = crate::sidebar::stash::list_stashes(repo)?;
    let same = current.len() == target.stashes.len()
        && current.iter().zip(&target.stashes).all(|(c, (id, _))| c.id == id.to_string());
    if same {
        return Ok(());
    }
    for _ in 0..current.len() {
        repo.stash_drop(0).map_err(err)?;
    }
    for (id, message) in target.stashes.iter().rev() {
        crate::toolbar::sync::run_git_with(
            path,
            &["stash", "store", "-m", message, &id.to_string()],
            Some(Duration::from_secs(30)),
        )?;
    }
    Ok(())
}

/// Moves the repository from the `expected` state to `target`.
fn apply(path: &str, kind: Kind, target: &Snapshot, expected: &Snapshot) -> Result<(), Failure> {
    let (mut repo, _) = open(path)?;
    let current = snapshot(&mut repo, kind)?;
    if !matches(&current, expected) {
        return Err(Failure::Diverged);
    }
    match kind {
        Kind::Keep => {}
        Kind::Switch => switch_files(&repo, target)?,
        Kind::Index => {
            let tree = target.index.ok_or_else(|| "The index state was not captured".to_string())?;
            let mut index = repo.index().map_err(err)?;
            index.read_tree(&repo.find_tree(tree).map_err(err)?).map_err(err)?;
            index.write().map_err(err)?;
        }
        Kind::Full => {
            let (Some(workdir), Some(index)) = (target.workdir, target.index) else {
                return Err(Failure::Other("The working directory state was not captured".into()));
            };
            restore_workdir(&repo, workdir, index)?;
        }
    }
    restore_refs(&repo, target)?;
    restore_stashes(path, &mut repo, target)?;
    Ok(())
}

fn run(path: &str, redo: bool) -> Result<String, String> {
    let (_, key) = open(path)?;
    let entry = {
        let journals = JOURNALS.lock().unwrap();
        let journal = journals.get(&key);
        let stack = journal.map(|j| if redo { &j.redo } else { &j.undo });
        stack.and_then(|s| s.last().cloned())
    }
    .ok_or(if redo { "Nothing to redo" } else { "Nothing to undo" })?;

    let (target, expected) = if redo { (&entry.after, &entry.before) } else { (&entry.before, &entry.after) };
    match apply(path, entry.kind, target, expected) {
        Ok(()) => {
            let mut journals = JOURNALS.lock().unwrap();
            let journal = journals.entry(key).or_default();
            let (from, to) =
                if redo { (&mut journal.redo, &mut journal.undo) } else { (&mut journal.undo, &mut journal.redo) };
            if let Some(done) = from.pop() {
                to.push(done);
            }
            Ok(entry.label)
        }
        Err(Failure::Diverged) => {
            JOURNALS.lock().unwrap().remove(&key);
            Err(format!(
                "The repository has changed since \"{}\" (outside the app, or by something that can't be undone), so \
                 nothing was changed and the undo history was cleared.",
                entry.label
            ))
        }
        Err(Failure::Other(e)) => Err(e),
    }
}

#[derive(Serialize, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UndoState {
    /// What Undo would take back, e.g. "Merge feature".
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
}

fn state(path: &str) -> Result<UndoState, String> {
    let (_, key) = open(path)?;
    let journals = JOURNALS.lock().unwrap();
    Ok(journals
        .get(&key)
        .map(|j| UndoState {
            undo_label: j.undo.last().map(|e| e.label.clone()),
            redo_label: j.redo.last().map(|e| e.label.clone()),
        })
        .unwrap_or_default())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_undo_state(path: String) -> Result<UndoState, String> {
    blocking(move || state(&path)).await
}

/// Takes back the last recorded action; resolves to its label.
#[tauri::command]
pub async fn undo_cmd(path: String) -> Result<String, String> {
    blocking(move || run(&path, false)).await
}

/// Repeats the action that was last taken back; resolves to its label.
#[tauri::command]
pub async fn redo_cmd(path: String) -> Result<String, String> {
    blocking(move || run(&path, true)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{commit_file, new_repo};
    use git2::{ResetType, Signature};
    use std::fs;

    /// Runs `op` as a recorded action.
    fn act(repo_dir: &Path, label: &str, kind: Kind, op: impl FnOnce()) {
        let p = repo_dir.to_str().unwrap();
        let before = begin(p, kind);
        op();
        finish(p, label.to_string(), kind, before);
    }

    fn read(dir: &Path, name: &str) -> String {
        fs::read_to_string(dir.join(name)).unwrap()
    }

    fn head(repo: &Repository) -> Oid {
        repo.head().unwrap().peel_to_commit().unwrap().id()
    }

    fn undo(dir: &Path) -> Result<String, String> {
        run(dir.to_str().unwrap(), false)
    }

    fn redo(dir: &Path) -> Result<String, String> {
        run(dir.to_str().unwrap(), true)
    }

    fn labels(dir: &Path) -> (Option<String>, Option<String>) {
        let s = state(dir.to_str().unwrap()).unwrap();
        (s.undo_label, s.redo_label)
    }

    #[test]
    fn a_commit_is_undone_with_its_changes_staged_and_redone() {
        let (dir, repo) = new_repo("undo-commit");
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        // Stage a change, then commit it as a recorded action.
        fs::write(dir.join("f.txt"), "b").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("f.txt")).unwrap();
        index.write().unwrap();
        let mut b = a;
        act(&dir, "Commit", Kind::Keep, || b = commit_staged_for_test(&repo, "B"));
        assert_eq!(head(&repo), b);
        assert_eq!(labels(&dir), (Some("Commit".into()), None));

        assert_eq!(undo(&dir).unwrap(), "Commit");
        assert_eq!(head(&repo), a);
        assert_eq!(read(&dir, "f.txt"), "b"); // the file is untouched
                                              // The change shows as staged against the old tip (the index still holds the committed content).
        let staged = repo.statuses(None).unwrap().iter().any(|s| s.status().is_index_modified());
        assert!(staged);
        assert_eq!(labels(&dir), (None, Some("Commit".into())));

        assert_eq!(redo(&dir).unwrap(), "Commit");
        assert_eq!(head(&repo), b);
        assert_eq!(labels(&dir), (Some("Commit".into()), None));
        let _ = fs::remove_dir_all(&dir);
    }

    fn commit_staged_for_test(repo: &Repository, msg: &str) -> Oid {
        let mut index = repo.index().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("T", "t@example.com").unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&parent]).unwrap()
    }

    #[test]
    fn the_first_commit_can_be_undone() {
        let (dir, repo) = new_repo("undo-first");
        let mut id = None;
        act(&dir, "Commit", Kind::Keep, || id = Some(commit_file(&repo, &dir, "f.txt", "a", "A")));
        assert!(id.is_some() && repo.head().is_ok());
        undo(&dir).unwrap();
        // Back to a branch without commits; the file is still there.
        assert!(repo.head().is_err());
        assert_eq!(read(&dir, "f.txt"), "a");
        redo(&dir).unwrap();
        assert_eq!(Some(head(&repo)), id);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hard_reset_gets_its_uncommitted_work_back() {
        let (dir, repo) = new_repo("undo-hard");
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        commit_file(&repo, &dir, "g.txt", "g", "G");
        // Uncommitted work: an edit, a staged new file and an untracked file.
        fs::write(dir.join("f.txt"), "edited").unwrap();
        fs::write(dir.join("s.txt"), "staged").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("s.txt")).unwrap();
        index.write().unwrap();
        fs::write(dir.join("u.txt"), "untracked").unwrap();
        let g = head(&repo);

        act(&dir, "Reset (hard)", Kind::Full, || {
            let target = repo.find_object(a, None).unwrap();
            repo.reset(&target, ResetType::Hard, None).unwrap();
            let _ = fs::remove_file(dir.join("u.txt")); // what a clean-up of untracked files would do
        });
        assert_eq!(head(&repo), a);
        assert_eq!(read(&dir, "f.txt"), "a");
        assert!(!dir.join("g.txt").exists() && !dir.join("s.txt").exists() && !dir.join("u.txt").exists());

        undo(&dir).unwrap();
        assert_eq!(head(&repo), g);
        assert_eq!(read(&dir, "f.txt"), "edited");
        assert_eq!(read(&dir, "s.txt"), "staged");
        assert_eq!(read(&dir, "u.txt"), "untracked");
        assert_eq!(read(&dir, "g.txt"), "g");
        // s.txt is staged again, f.txt is an unstaged edit.
        let st = |name: &str| repo.status_file(Path::new(name)).unwrap();
        assert!(st("s.txt").is_index_new());
        assert!(st("f.txt").is_wt_modified());

        redo(&dir).unwrap();
        assert_eq!(head(&repo), a);
        assert!(!dir.join("s.txt").exists() && !dir.join("u.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_discard_is_undone_including_untracked_files() {
        let (dir, repo) = new_repo("undo-discard");
        commit_file(&repo, &dir, "f.txt", "a", "A");
        fs::write(dir.join("f.txt"), "precious").unwrap();
        fs::write(dir.join("new.txt"), "also precious").unwrap();

        act(&dir, "Discard changes", Kind::Full, || {
            fs::write(dir.join("f.txt"), "a").unwrap();
            fs::remove_file(dir.join("new.txt")).unwrap();
        });
        assert_eq!(read(&dir, "f.txt"), "a");
        undo(&dir).unwrap();
        assert_eq!(read(&dir, "f.txt"), "precious");
        assert_eq!(read(&dir, "new.txt"), "also precious");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stash_actions_are_undone_with_the_stash_list_and_the_files() {
        let (dir, mut repo) = new_repo("undo-stash");
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "T").unwrap();
        cfg.set_str("user.email", "t@example.com").unwrap();
        commit_file(&repo, &dir, "f.txt", "a", "A");
        fs::write(dir.join("f.txt"), "work").unwrap();
        fs::write(dir.join("u.txt"), "untracked").unwrap();

        // Create: undo puts the files back and removes the stash.
        act(&dir, "Stash changes", Kind::Full, || {
            crate::sidebar::stash::save_stash(&mut repo, Some("mine")).unwrap();
        });
        assert_eq!(read(&dir, "f.txt"), "a");
        assert_eq!(crate::sidebar::stash::list_stashes(&mut repo).unwrap().len(), 1);
        undo(&dir).unwrap();
        assert_eq!(read(&dir, "f.txt"), "work");
        assert_eq!(read(&dir, "u.txt"), "untracked");
        assert!(crate::sidebar::stash::list_stashes(&mut repo).unwrap().is_empty());

        // Redo stashes again; then a pop is undone: the stash returns (same commit, same message), the files go.
        redo(&dir).unwrap();
        let stash = crate::sidebar::stash::list_stashes(&mut repo).unwrap().remove(0);
        act(&dir, "Pop stash", Kind::Full, || {
            repo.stash_pop(0, None).unwrap();
        });
        assert_eq!(read(&dir, "f.txt"), "work");
        undo(&dir).unwrap();
        assert_eq!(read(&dir, "f.txt"), "a");
        assert!(!dir.join("u.txt").exists());
        let list = crate::sidebar::stash::list_stashes(&mut repo).unwrap();
        assert_eq!(
            (list.len(), list[0].id.as_str(), list[0].message.as_str()),
            (1, stash.id.as_str(), stash.message.as_str())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn branches_and_checkouts_are_undone() {
        let (dir, repo) = new_repo("undo-branch");
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        let main = repo.head().unwrap().shorthand().unwrap().to_string();

        // Create and switch to a branch, then delete another: both are taken back, newest first.
        act(&dir, "Create branch x", Kind::Keep, || {
            repo.branch("x", &repo.find_commit(a).unwrap(), false).unwrap();
            repo.set_head("refs/heads/x").unwrap();
        });
        act(&dir, "Check out main", Kind::Switch, || repo.set_head(&format!("refs/heads/{main}")).unwrap());
        act(&dir, "Delete branch x", Kind::Keep, || {
            repo.find_branch("x", BranchType::Local).unwrap().delete().unwrap()
        });
        assert!(repo.find_branch("x", BranchType::Local).is_err());

        assert_eq!(undo(&dir).unwrap(), "Delete branch x");
        assert!(repo.find_branch("x", BranchType::Local).is_ok());
        assert_eq!(undo(&dir).unwrap(), "Check out main");
        assert_eq!(repo.head().unwrap().shorthand().unwrap(), "x");
        assert_eq!(undo(&dir).unwrap(), "Create branch x");
        assert!(repo.find_branch("x", BranchType::Local).is_err());
        assert_eq!(repo.head().unwrap().shorthand().unwrap(), main);
        assert!(undo(&dir).unwrap_err().contains("Nothing to undo"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fast_forward_is_undone_on_disk_but_not_over_newer_edits() {
        let (dir, repo) = new_repo("undo-switch");
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        let main_ref = repo.head().unwrap().name().unwrap().to_string();
        repo.branch("feature", &repo.find_commit(a).unwrap(), false).unwrap();
        repo.set_head("refs/heads/feature").unwrap();
        let b = commit_file(&repo, &dir, "f.txt", "b", "B");
        repo.set_head(&main_ref).unwrap();
        repo.checkout_head(Some(CheckoutBuilder::new().force())).unwrap();

        act(&dir, "Fast-forward", Kind::Switch, || {
            let tree = repo.find_commit(b).unwrap().tree().unwrap();
            repo.checkout_tree(tree.as_object(), Some(CheckoutBuilder::new().safe())).unwrap();
            repo.reference(&main_ref, b, true, "ff").unwrap();
        });
        assert_eq!(read(&dir, "f.txt"), "b");

        // The user edits the file the fast-forward changed: undo must not overwrite that.
        fs::write(dir.join("f.txt"), "mine").unwrap();
        let e = undo(&dir).unwrap_err();
        assert!(e.contains("Changes you made"), "{e}");
        assert_eq!(head(&repo), b);
        assert_eq!(read(&dir, "f.txt"), "mine");

        // Without the edit it works, and the entry was kept.
        fs::write(dir.join("f.txt"), "b").unwrap();
        undo(&dir).unwrap();
        assert_eq!(head(&repo), a);
        assert_eq!(read(&dir, "f.txt"), "a");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn staging_is_undone_and_changes_made_elsewhere_stop_the_history() {
        let (dir, repo) = new_repo("undo-index");
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        fs::write(dir.join("f.txt"), "b").unwrap();
        act(&dir, "Stage f.txt", Kind::Index, || {
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("f.txt")).unwrap();
            index.write().unwrap();
        });
        assert!(repo.status_file(Path::new("f.txt")).unwrap().is_index_modified());
        undo(&dir).unwrap();
        let st = repo.status_file(Path::new("f.txt")).unwrap();
        assert!(!st.is_index_modified() && st.is_wt_modified());

        // Redo, then something outside the app moves the branch: the next undo refuses and clears the history.
        redo(&dir).unwrap();
        let tree = repo.find_commit(a).unwrap().tree().unwrap();
        let sig = Signature::now("T", "t@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "elsewhere", &tree, &[&repo.find_commit(a).unwrap()]).unwrap();
        let e = undo(&dir).unwrap_err();
        assert!(e.contains("has changed since") && e.contains("history was cleared"), "{e}");
        assert_eq!(labels(&dir), (None, None));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_is_recorded_when_nothing_changed_and_a_new_action_ends_redo() {
        let (dir, repo) = new_repo("undo-noop");
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        act(&dir, "Nothing", Kind::Keep, || {});
        assert_eq!(labels(&dir), (None, None));

        repo.branch("x", &repo.find_commit(a).unwrap(), false).unwrap();
        act(&dir, "Delete x", Kind::Keep, || repo.find_branch("x", BranchType::Local).unwrap().delete().unwrap());
        undo(&dir).unwrap();
        assert_eq!(labels(&dir), (None, Some("Delete x".into())));
        act(&dir, "Delete x again", Kind::Keep, || repo.find_branch("x", BranchType::Local).unwrap().delete().unwrap());
        assert_eq!(labels(&dir), (Some("Delete x again".into()), None));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn describes_paths_for_labels() {
        assert_eq!(describe_paths(&["src/a/b.txt".into()]), "b.txt");
        assert_eq!(describe_paths(&["a".into(), "b".into(), "c".into()]), "3 files");
        assert_eq!(short_ref("refs/heads/feature/x"), "feature/x");
        assert_eq!(short_ref("refs/remotes/origin/main"), "origin/main");
        assert_eq!(short_ref("0123456789abcdef"), "0123456");
    }

    #[test]
    fn recorded_wraps_an_async_action_and_skips_failed_ones() {
        let (dir, repo) = new_repo("undo-recorded");
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        repo.branch("x", &repo.find_commit(a).unwrap(), false).unwrap();
        let p = dir.to_str().unwrap().to_string();

        let deleted = tauri::async_runtime::block_on(recorded(&p, "Delete branch x", Kind::Keep, || async {
            tauri::async_runtime::spawn_blocking({
                let p = p.clone();
                move || {
                    let repo = Repository::discover(&p).map_err(err)?;
                    let mut branch = repo.find_branch("x", BranchType::Local).map_err(err)?;
                    branch.delete().map_err(err)
                }
            })
            .await
            .map_err(|e| e.to_string())?
        }));
        assert!(deleted.is_ok());
        assert_eq!(labels(&dir), (Some("Delete branch x".into()), None));

        // A failing action leaves no entry behind.
        let failed: Result<(), String> =
            tauri::async_runtime::block_on(recorded(&p, "Broken", Kind::Keep, || async { Err("nope".to_string()) }));
        assert!(failed.is_err());
        assert_eq!(labels(&dir), (Some("Delete branch x".into()), None));
        let _ = fs::remove_dir_all(&dir);
    }
}
