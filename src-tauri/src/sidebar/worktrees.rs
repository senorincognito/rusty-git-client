use std::path::Path;

use git2::{BranchType, Repository};
use serde::Serialize;

use crate::toolbar::sync::run_git;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    /// Same form as `RepoInfo.path`, so it can be opened as a tab as it is.
    pub path: String,
    pub name: String,
    /// Checked-out branch, None when detached.
    pub branch: Option<String>,
    /// Short id of the commit HEAD points at.
    pub head: String,
    pub detached: bool,
    /// The repository's own working directory (git lists it first); it can't be removed.
    pub is_main: bool,
    /// The worktree the open tab shows.
    pub is_current: bool,
    pub locked: bool,
    /// The folder is gone (deleted by hand): `prune` forgets it.
    pub missing: bool,
}

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

/// Canonical path without the Windows `\\?\` prefix; the path as given when the folder doesn't exist.
fn normalize(path: &str) -> String {
    match Path::new(path).canonicalize() {
        Ok(p) => p.to_string_lossy().trim_start_matches(r"\\?\").to_string(),
        Err(_) => path.to_string(),
    }
}

/// One record of `git worktree list --porcelain` (blank-line separated, `key value` lines).
#[derive(Default, Debug, PartialEq)]
struct Raw {
    path: String,
    head: String,
    branch: Option<String>,
    detached: bool,
    bare: bool,
    locked: bool,
    prunable: bool,
}

fn parse_porcelain(out: &str) -> Vec<Raw> {
    let mut list = Vec::new();
    let mut cur: Option<Raw> = None;
    for line in out.lines() {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "worktree" => {
                list.extend(cur.take());
                cur = Some(Raw { path: value.to_string(), ..Raw::default() });
            }
            "HEAD" => cur.iter_mut().for_each(|r| r.head = value.chars().take(7).collect()),
            "branch" => cur
                .iter_mut()
                .for_each(|r| r.branch = Some(value.strip_prefix("refs/heads/").unwrap_or(value).to_string())),
            "detached" => cur.iter_mut().for_each(|r| r.detached = true),
            "bare" => cur.iter_mut().for_each(|r| r.bare = true),
            "locked" => cur.iter_mut().for_each(|r| r.locked = true),
            "prunable" => cur.iter_mut().for_each(|r| r.prunable = true),
            _ => {}
        }
    }
    list.extend(cur);
    list
}

/// Every worktree of the repository `path` belongs to, the main one first, the rest sorted by name.
fn list_worktrees(path: &str) -> Result<Vec<WorktreeInfo>, String> {
    let current = Repository::discover(path).map_err(err)?.workdir().map(|w| normalize(&w.to_string_lossy()));
    let out = run_git(path, &["worktree", "list", "--porcelain"])?;
    let mut list: Vec<WorktreeInfo> = parse_porcelain(&out)
        .into_iter()
        .enumerate()
        .filter(|(_, r)| !r.bare)
        .map(|(i, r)| {
            let missing = !Path::new(&r.path).exists();
            let path = normalize(&r.path);
            let name =
                Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
            WorktreeInfo {
                is_current: current.as_deref() == Some(path.as_str()),
                path,
                name,
                detached: r.detached || r.branch.is_none(),
                branch: r.branch,
                head: r.head,
                is_main: i == 0,
                locked: r.locked,
                missing: missing || r.prunable,
            }
        })
        .collect();
    let rest = 1.min(list.len());
    list[rest..].sort_by_key(|w| w.name.to_lowercase());
    Ok(list)
}

/// Where a worktree for `branch` goes: `<repo>.worktrees/<branch>` next to the main working directory, so it never
/// shows up as untracked files inside the repository. Slashes of the branch name become dashes.
fn target_dir(main: &str, branch: &str) -> std::path::PathBuf {
    let main = Path::new(main);
    let name = main.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "repo".into());
    let parent = main.parent().unwrap_or(main);
    parent.join(format!("{name}.worktrees")).join(branch.replace('/', "-"))
}

/// Adds a worktree with `branch` checked out. A branch that exists is used as it is (git refuses one that another
/// worktree has checked out); otherwise it is created from the commit HEAD points at. Returns the new path.
fn add_worktree(path: &str, branch: &str) -> Result<String, String> {
    let branch = crate::sidebar::branches::validate_branch_name(branch)?;
    let repo = Repository::discover(path).map_err(err)?;
    let exists = repo.find_branch(branch, BranchType::Local).is_ok();
    let main =
        list_worktrees(path)?.into_iter().find(|w| w.is_main).ok_or("Could not find the main working directory")?;
    let dir = target_dir(&main.path, branch);
    if dir.exists() {
        return Err(format!("The folder {} already exists", dir.display()));
    }
    let dir_arg = dir.to_string_lossy().to_string();
    if exists {
        run_git(path, &["worktree", "add", &dir_arg, branch])?;
    } else {
        run_git(path, &["worktree", "add", "-b", branch, &dir_arg])?;
    }
    Ok(normalize(&dir_arg))
}

/// Removes a linked worktree (its folder and git's bookkeeping; the branch stays). Without `force` git refuses a
/// worktree with modified or untracked files, which the caller can turn into a confirmation.
fn remove_worktree(path: &str, worktree: &str, force: bool) -> Result<(), String> {
    let target = normalize(worktree);
    let found = list_worktrees(path)?.into_iter().find(|w| w.path == target).ok_or("This worktree no longer exists")?;
    if found.is_main {
        return Err("The main working directory can't be removed".into());
    }
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&found.path);
    run_git(path, &args).map(|_| ())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_worktrees(path: String) -> Result<Vec<WorktreeInfo>, String> {
    blocking(move || list_worktrees(&path)).await
}

#[tauri::command]
pub async fn add_worktree_cmd(path: String, branch: String) -> Result<String, String> {
    blocking(move || add_worktree(&path, &branch)).await
}

#[tauri::command]
pub async fn remove_worktree_cmd(path: String, worktree: String, force: bool) -> Result<(), String> {
    blocking(move || remove_worktree(&path, &worktree, force)).await
}

/// Forgets worktrees whose folder was deleted by hand.
#[tauri::command]
pub async fn prune_worktrees_cmd(path: String) -> Result<(), String> {
    blocking(move || run_git(&path, &["worktree", "prune"]).map(|_| ())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// A repository with one commit on `main`, inside a fresh folder that also receives the worktrees.
    fn setup(name: &str) -> (std::path::PathBuf, String) {
        let root = std::env::temp_dir().join(format!("gc-wt-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "a").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "a"]);
        let path = normalize(&repo.to_string_lossy());
        (root, path)
    }

    #[test]
    fn parses_porcelain_output() {
        let out = "worktree C:/r\nHEAD 0123456789abcdef\nbranch refs/heads/main\n\n\
                   worktree C:/r.worktrees/x\nHEAD fedcba9876543210\ndetached\nlocked reason\n\n\
                   worktree C:/gone\nHEAD 1111111111111111\nbranch refs/heads/feat/y\nprunable gitdir file points to non-existent location\n";
        let list = parse_porcelain(out);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].branch.as_deref(), Some("main"));
        assert_eq!(list[0].head, "0123456");
        assert!(list[1].detached && list[1].locked && list[1].branch.is_none());
        assert_eq!(list[2].branch.as_deref(), Some("feat/y"));
        assert!(list[2].prunable);
    }

    #[test]
    fn adds_lists_and_removes_a_worktree() {
        let (root, path) = setup("cycle");
        let new = add_worktree(&path, "feature/one").unwrap();
        assert!(new.ends_with("feature-one"), "{new}");
        assert!(Path::new(&new).join("a.txt").exists());

        let list = list_worktrees(&path).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0].is_main && list[0].is_current && list[0].branch.as_deref() == Some("main"));
        assert!(!list[1].is_main && !list[1].is_current && list[1].branch.as_deref() == Some("feature/one"));
        assert_eq!(list[1].path, new);

        // Seen from the linked worktree, it is the current one.
        let from_linked = list_worktrees(&new).unwrap();
        assert!(from_linked.iter().any(|w| w.is_current && w.path == new));

        remove_worktree(&path, &new, false).unwrap();
        assert!(!Path::new(&new).exists());
        assert_eq!(list_worktrees(&path).unwrap().len(), 1);
        // The branch outlives its worktree.
        assert!(Repository::open(&path).unwrap().find_branch("feature/one", BranchType::Local).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn uses_an_existing_branch_but_not_one_that_is_checked_out() {
        let (root, path) = setup("existing");
        git(Path::new(&path), &["branch", "topic"]);
        let new = add_worktree(&path, "topic").unwrap();
        assert!(add_worktree(&path, "topic").is_err(), "a branch can only be checked out once");
        assert!(add_worktree(&path, "main").is_err());
        assert!(add_worktree(&path, "bad name").is_err());
        remove_worktree(&path, &new, false).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_dirty_worktrees_unless_forced_and_never_removes_the_main_one() {
        let (root, path) = setup("dirty");
        let new = add_worktree(&path, "wip").unwrap();
        std::fs::write(Path::new(&new).join("untracked.txt"), "x").unwrap();
        assert!(remove_worktree(&path, &new, false).is_err());
        assert!(Path::new(&new).exists());
        remove_worktree(&path, &new, true).unwrap();
        assert!(!Path::new(&new).exists());
        assert!(remove_worktree(&path, &path, true).unwrap_err().contains("main"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn marks_a_deleted_folder_missing_until_pruned() {
        let (root, path) = setup("prune");
        let new = add_worktree(&path, "gone").unwrap();
        std::fs::remove_dir_all(&new).unwrap();
        let list = list_worktrees(&path).unwrap();
        assert!(list.iter().any(|w| w.branch.as_deref() == Some("gone") && w.missing));
        run_git(&path, &["worktree", "prune"]).unwrap();
        assert_eq!(list_worktrees(&path).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }
}
