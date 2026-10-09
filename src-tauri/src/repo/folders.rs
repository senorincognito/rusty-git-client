//! The folder of repositories shown on the start screen: remembered across sessions, scanned for repositories.

use std::fs;
use std::path::{Path, PathBuf};

use git2::Repository;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::{describe, RepoInfo};

/// How many levels below the chosen folder are searched (1 = only its direct children).
const MAX_DEPTH: usize = 3;
/// A safety net for a folder like the home directory.
const MAX_REPOS: usize = 500;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FolderRepo {
    #[serde(flatten)]
    pub info: RepoInfo,
    /// The folders between the chosen one and the repository ("" for a direct child), with `/`.
    pub parent: String,
    /// Unix seconds of the commit HEAD points at; None for a repository without commits.
    pub last_commit: Option<i64>,
}

fn folder_file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("repo_folder.json"))
}

/// The remembered folder, if it still exists.
#[tauri::command]
pub fn get_repo_folder(app: AppHandle) -> Option<String> {
    let bytes = fs::read(folder_file(&app).ok()?).ok()?;
    let path: String = serde_json::from_slice(&bytes).ok()?;
    Path::new(&path).is_dir().then_some(path)
}

/// Remembers the folder (None forgets it).
#[tauri::command]
pub fn set_repo_folder(app: AppHandle, path: Option<String>) -> Result<(), String> {
    let file = folder_file(&app)?;
    match path {
        Some(p) => {
            if !Path::new(&p).is_dir() {
                return Err(format!("\"{p}\" is not a folder"));
            }
            fs::write(file, serde_json::to_vec(&p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        }
        None => match fs::remove_file(file) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        },
    }
}

fn repo_at(dir: &Path, root: &Path) -> Option<FolderRepo> {
    let repo = Repository::open(dir).ok()?;
    if repo.is_bare() {
        return None;
    }
    let info = describe(&repo).ok()?;
    let last_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok()).map(|c| c.time().seconds());
    let parent = dir
        .parent()
        .and_then(|p| p.strip_prefix(root).ok())
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    Some(FolderRepo { info, parent, last_commit })
}

fn walk(dir: &Path, root: &Path, depth: usize, out: &mut Vec<FolderRepo>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if out.len() >= MAX_REPOS {
            return;
        }
        // Symlinks are not followed, so a link back up the tree can't loop.
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        let path = entry.path();
        if path.join(".git").exists() {
            // A repository is listed, not searched: what is inside it belongs to it.
            out.extend(repo_at(&path, root));
        } else if depth < MAX_DEPTH {
            walk(&path, root, depth + 1, out);
        }
    }
}

/// Every repository inside `root` (up to [`MAX_DEPTH`] levels down), sorted by name. Hidden folders and
/// `node_modules` are skipped.
fn scan(root: &Path) -> Result<Vec<FolderRepo>, String> {
    if !root.is_dir() {
        return Err(format!("\"{}\" is not a folder", root.display()));
    }
    let mut out = Vec::new();
    if root.join(".git").exists() {
        out.extend(repo_at(root, root.parent().unwrap_or(root)));
    } else {
        walk(root, root, 1, &mut out);
    }
    out.sort_by(|a, b| {
        a.info.name.to_lowercase().cmp(&b.info.name.to_lowercase()).then_with(|| a.info.path.cmp(&b.info.path))
    });
    Ok(out)
}

/// Lists the repositories inside a folder.
#[tauri::command]
pub async fn scan_repo_folder(path: String) -> Result<Vec<FolderRepo>, String> {
    tauri::async_runtime::spawn_blocking(move || scan(Path::new(&path))).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init(path: &Path) {
        fs::create_dir_all(path).unwrap();
        Repository::init(path).unwrap();
    }

    #[test]
    fn finds_repositories_at_several_depths_and_skips_the_rest() {
        let root = std::env::temp_dir().join(format!("gc-folder-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        init(&root.join("alpha"));
        init(&root.join("Zeta"));
        init(&root.join("group").join("beta"));
        init(&root.join("group").join("deeper").join("gamma"));
        init(&root.join("a").join("b").join("c").join("too-deep"));
        init(&root.join("alpha").join("inside-a-repo"));
        init(&root.join("node_modules").join("pkg"));
        init(&root.join(".hidden").join("secret"));
        fs::create_dir_all(root.join("empty")).unwrap();
        fs::write(root.join("file.txt"), "x").unwrap();

        let found = scan(&root).unwrap();
        let names: Vec<_> = found.iter().map(|r| r.info.name.as_str()).collect();
        assert_eq!(names, ["alpha", "beta", "gamma", "Zeta"]);
        let parent = |n: &str| found.iter().find(|r| r.info.name == n).unwrap().parent.clone();
        assert_eq!(
            (parent("alpha").as_str(), parent("beta").as_str(), parent("gamma").as_str()),
            ("", "group", "group/deeper")
        );
        // A fresh repository has no commits and no current branch yet.
        assert!(found.iter().all(|r| r.last_commit.is_none() && r.info.head.is_none()));

        assert!(scan(&root.join("file.txt")).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_folder_that_is_itself_a_repository_lists_just_that() {
        let root = std::env::temp_dir().join(format!("gc-folder-self-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        init(&root);
        init(&root.join("sub"));
        let found = scan(&root).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].parent, "");
        let _ = fs::remove_dir_all(&root);
    }
}
