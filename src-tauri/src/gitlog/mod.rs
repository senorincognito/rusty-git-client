//! A log of every call to the system `git`, for the command log view. Only real processes are recorded
//! (`toolbar::sync::run_git*` is the one place that spawns git): operations done through libgit2 (stage, checkout,
//! branches, stashes...) run no command and so never appear.
//!
//! The log is kept in memory (the last [`MAX_ENTRIES`] calls) whether or not the view is open, so opening it shows
//! what already happened. Each call is published as a `git-log` event when it starts and again when it ends
//! (same `id`), so a slow fetch shows up while it runs.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

const MAX_ENTRIES: usize = 500;
/// Output kept per call: enough to read a fetch or an error, not a whole `ls-files`.
const MAX_OUTPUT: usize = 2000;

static LOG: Mutex<VecDeque<GitLogEntry>> = Mutex::new(VecDeque::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static APP: OnceLock<AppHandle> = OnceLock::new();

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GitLogEntry {
    pub id: u64,
    /// When git was started, in milliseconds since the Unix epoch.
    pub time: u64,
    /// The repository folder (`git -C <path>`).
    pub path: String,
    /// The arguments after `-C <path>`.
    pub args: Vec<String>,
    pub running: bool,
    pub ok: bool,
    pub duration_ms: u64,
    /// stdout and stderr (or the error), cut to [`MAX_OUTPUT`] characters.
    pub output: String,
}

/// Remembers the handle the `git-log` events are emitted through.
pub fn start(app: AppHandle) {
    let _ = APP.set(app);
}

fn log() -> std::sync::MutexGuard<'static, VecDeque<GitLogEntry>> {
    LOG.lock().unwrap_or_else(|e| e.into_inner())
}

fn publish(entry: &GitLogEntry) {
    if let Some(app) = APP.get() {
        let _ = app.emit("git-log", entry);
    }
}

/// Records that `git <args>` is starting in `path`; returns the id to pass to [`finish`].
pub(crate) fn begin(path: &str, args: &[&str]) -> u64 {
    let entry = GitLogEntry {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        time: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64),
        path: path.to_string(),
        args: args.iter().map(|a| a.to_string()).collect(),
        running: true,
        ok: false,
        duration_ms: 0,
        output: String::new(),
    };
    {
        let mut log = log();
        log.push_back(entry.clone());
        while log.len() > MAX_ENTRIES {
            log.pop_front();
        }
    }
    publish(&entry);
    entry.id
}

/// Completes the call `id` with git's result (`Err` holds its message too).
pub(crate) fn finish(id: u64, result: &Result<String, String>, took: Duration) {
    let (ok, text) = match result {
        Ok(text) => (true, text),
        Err(text) => (false, text),
    };
    let mut output: String = text.chars().take(MAX_OUTPUT).collect();
    if text.chars().count() > MAX_OUTPUT {
        output.push('…');
    }
    let done = {
        let mut log = log();
        let Some(entry) = log.iter_mut().rev().find(|e| e.id == id) else { return };
        entry.running = false;
        entry.ok = ok;
        entry.duration_ms = took.as_millis() as u64;
        entry.output = output;
        entry.clone()
    };
    publish(&done);
}

/// The recorded calls, oldest first.
#[tauri::command]
pub async fn get_git_log() -> Vec<GitLogEntry> {
    log().iter().cloned().collect()
}

#[tauri::command]
pub async fn clear_git_log() {
    log().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64) -> Option<GitLogEntry> {
        log().iter().find(|e| e.id == id).cloned()
    }

    #[test]
    fn a_call_is_recorded_when_it_starts_and_completed_when_it_ends() {
        let id = begin("/repo", &["fetch", "--all"]);
        let started = entry(id).unwrap();
        assert!(started.running);
        assert_eq!(started.args, ["fetch", "--all"]);
        assert_eq!(started.path, "/repo");

        finish(id, &Err("fatal: nope".into()), Duration::from_millis(42));
        let done = entry(id).unwrap();
        assert!(!done.running && !done.ok);
        assert_eq!(done.duration_ms, 42);
        assert_eq!(done.output, "fatal: nope");
    }

    #[test]
    fn long_output_is_cut() {
        let id = begin("/repo", &["ls-files"]);
        finish(id, &Ok("x".repeat(MAX_OUTPUT + 500)), Duration::ZERO);
        let output = entry(id).unwrap().output;
        assert_eq!(output.chars().count(), MAX_OUTPUT + 1);
        assert!(output.ends_with('…'));
    }

    #[test]
    fn only_the_latest_calls_are_kept() {
        for _ in 0..MAX_ENTRIES + 10 {
            begin("/repo", &["status"]);
        }
        assert!(log().len() <= MAX_ENTRIES);
    }

    #[test]
    fn run_git_logs_the_process_it_starts() {
        let dir = env!("CARGO_MANIFEST_DIR");
        crate::toolbar::sync::run_git(dir, &["--version"]).unwrap();
        let logged = log().iter().rev().find(|e| e.args == ["--version"] && e.path == dir).cloned().unwrap();
        assert!(logged.ok && !logged.running);
        assert!(logged.output.starts_with("git version"));
    }
}
