pub mod folders;
pub mod watch;

use std::fs;
use std::path::{Path, PathBuf};

use git2::Repository;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const MAX_RECENT: usize = 20;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub path: String,
    pub name: String,
    /// Current branch name, or short commit id when HEAD is detached, or None for an unborn branch.
    pub head: Option<String>,
    pub detached: bool,
}

fn recents_file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("recent_repos.json"))
}

fn load_recents(app: &AppHandle) -> Vec<RepoInfo> {
    recents_file(app)
        .ok()
        .and_then(|f| fs::read(f).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_recents(app: &AppHandle, list: &[RepoInfo]) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(list).map_err(|e| e.to_string())?;
    fs::write(recents_file(app)?, json).map_err(|e| e.to_string())
}

fn describe(repo: &Repository) -> Result<RepoInfo, String> {
    let root = repo.workdir().or_else(|| Some(repo.path())).ok_or("repository has no path")?;
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    // Strip the Windows verbatim prefix (\\?\) so paths stay readable.
    let path = root.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    let name = Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());

    let (head, detached) = match repo.head() {
        Ok(h) if h.is_branch() => (h.shorthand().ok().map(str::to_string), false),
        Ok(h) => (h.target().map(|oid| oid.to_string()[..7].to_string()), true),
        // Unborn branch (fresh `git init`): HEAD points at a ref that doesn't exist yet.
        Err(_) => (None, false),
    };

    Ok(RepoInfo { path, name, head, detached })
}

/// Opens the repo containing `path` (searching upward) and records it as most recent.
#[tauri::command]
pub fn open_repo(app: AppHandle, path: String) -> Result<RepoInfo, String> {
    let repo = Repository::discover(&path).map_err(|e| format!("Not a git repository: {path} ({})", e.message()))?;
    let info = describe(&repo)?;

    let mut recents = load_recents(&app);
    recents.retain(|r| r.path != info.path);
    recents.insert(0, info.clone());
    recents.truncate(MAX_RECENT);
    save_recents(&app, &recents)?;

    Ok(info)
}

/// Recent repos, newest first. Entries whose folder no longer exists are dropped.
#[tauri::command]
pub fn get_recent_repos(app: AppHandle) -> Vec<RepoInfo> {
    let mut recents = load_recents(&app);
    let before = recents.len();
    recents.retain(|r| Path::new(&r.path).exists());
    if recents.len() != before {
        let _ = save_recents(&app, &recents);
    }
    recents
}

#[tauri::command]
pub fn remove_recent_repo(app: AppHandle, path: String) -> Result<(), String> {
    let mut recents = load_recents(&app);
    recents.retain(|r| r.path != path);
    save_recents(&app, &recents)
}
