use std::sync::Mutex;
use std::time::Duration;

use git2::Repository;
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use tauri::{AppHandle, Emitter, State};

/// Holds the watcher for the currently open repo; replacing it stops the previous one.
#[derive(Default)]
pub struct RepoWatcher(Mutex<Option<Debouncer<RecommendedWatcher>>>);

/// Starts watching the repo's git directory and emits `repo-changed` (debounced) whenever
/// HEAD, the index or any ref changes, no matter which program made the change.
/// Object files are deliberately not watched: they churn during every commit.
#[tauri::command]
pub fn watch_repo(app: AppHandle, state: State<RepoWatcher>, path: String) -> Result<(), String> {
    let repo = Repository::discover(&path).map_err(|e| e.message().to_string())?;
    let git_dir = repo.path().to_path_buf();

    let mut debouncer = new_debouncer(Duration::from_millis(300), move |res: DebounceEventResult| {
        if res.is_ok() {
            let _ = app.emit("repo-changed", ());
        }
    })
    .map_err(|e| e.to_string())?;

    let w = debouncer.watcher();
    // HEAD, index, packed-refs, MERGE_HEAD, ...
    w.watch(&git_dir, RecursiveMode::NonRecursive).map_err(|e| e.to_string())?;
    // Branches, remotes and tags (the folder may be missing in a brand-new repo). A linked worktree's git directory
    // (`.git/worktrees/<name>`) holds only its own HEAD and index; the refs are shared in the common directory.
    let refs = repo.commondir().join("refs");
    if refs.exists() {
        w.watch(&refs, RecursiveMode::Recursive).map_err(|e| e.to_string())?;
    }

    *state.0.lock().unwrap() = Some(debouncer);
    Ok(())
}

#[tauri::command]
pub fn unwatch_repo(state: State<RepoWatcher>) {
    *state.0.lock().unwrap() = None;
}
