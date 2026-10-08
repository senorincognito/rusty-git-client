use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use git2::{BranchType, Repository, RepositoryState, Sort};
use serde::Serialize;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    /// Current local branch; None when HEAD is detached or unborn.
    pub branch: Option<String>,
    /// Upstream as "origin/main", if the branch tracks one.
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub has_remote: bool,
}

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

fn sync_status(repo: &Repository) -> Result<SyncStatus, String> {
    let has_remote = !repo.remotes().map_err(err)?.is_empty();
    let mut status = SyncStatus { branch: None, upstream: None, ahead: 0, behind: 0, has_remote };

    let Ok(head) = repo.head() else { return Ok(status) };
    if !head.is_branch() {
        return Ok(status);
    }
    let Ok(name) = head.shorthand().map(str::to_string) else { return Ok(status) };
    status.branch = Some(name.clone());

    if let Ok(local) = repo.find_branch(&name, BranchType::Local) {
        // No upstream configured -> find_upstream fails; that's a normal state.
        if let Ok(up) = local.upstream() {
            status.upstream = up.name().ok().flatten().map(str::to_string);
            if let (Some(l), Some(u)) = (head.target(), up.get().target()) {
                let (ahead, behind) = repo.graph_ahead_behind(l, u).map_err(err)?;
                status.ahead = ahead;
                status.behind = behind;
            }
        }
    }
    Ok(status)
}

/// Runs the system `git` so the user's credential helpers, SSH agent and config all apply. When git needs a
/// password, token or passphrase the user is asked in a dialog (see `auth`).
pub(crate) fn run_git(path: &str, args: &[&str]) -> Result<String, String> {
    run_git_inner(path, args, None, true)
}

/// Like [`run_git`], but kills git and fails if it takes longer than `timeout`, and never asks the user for
/// credentials (it serves background work such as auto-fetch).
pub(crate) fn run_git_with(path: &str, args: &[&str], timeout: Option<Duration>) -> Result<String, String> {
    run_git_inner(path, args, timeout, false)
}

fn run_git_inner(path: &str, args: &[&str], timeout: Option<Duration>, may_ask: bool) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(path)
        .args(args)
        // Fail instead of waiting for a password on a terminal that doesn't exist.
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if may_ask {
        crate::auth::apply_env(&mut cmd);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // own group, so a timeout can take down git's helpers too
    }

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "git executable not found. Install Git and make sure it is on your PATH.".to_string()
        } else {
            e.to_string()
        }
    })?;

    // Drain both pipes on their own threads so a chatty git can never block on a full pipe.
    fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    }
    let (out_pipe, err_pipe) = (child.stdout.take(), child.stderr.take());

    let out_reader = drain(out_pipe);
    let err_reader = drain(err_pipe);
    let deadline = timeout.map(|t| Instant::now() + t);
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break Some(status),
            None if deadline.is_some_and(|d| Instant::now() >= d) => {
                kill_tree(&mut child);
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(40)),
        }
    };
    let Some(status) = status else {
        return Err(format!(
            "git {} timed out after {}s",
            args.first().unwrap_or(&""),
            timeout.map_or(0, |t| t.as_secs())
        ));
    };
    // Only now that git has exited are the pipes guaranteed to close.
    let stdout = String::from_utf8_lossy(&out_reader.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).into_owned();

    // git reports progress and most messages on stderr.
    let text = format!("{}\n{}", stdout.trim(), stderr.trim()).trim().to_string();
    if status.success() {
        Ok(text)
    } else if text.is_empty() {
        Err(format!("git {} failed ({})", args.first().unwrap_or(&""), status))
    } else {
        Err(text)
    }
}

/// Kills `child` and everything it spawned (git starts helpers such as git-remote-https and
/// ssh, which would otherwise live on and keep the connection and our output pipes open).
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .output();
    }
    #[cfg(unix)]
    {
        // A negative pid addresses the whole process group created at spawn.
        let _ = Command::new("kill").args(["-KILL", &format!("-{}", child.id())]).output();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn open(path: &str) -> Result<Repository, String> {
    Repository::discover(path).map_err(err)
}

fn fetch(path: &str) -> Result<String, String> {
    if !sync_status(&open(path)?)?.has_remote {
        return Err("No remotes configured for this repository".into());
    }
    run_git(path, &["fetch", "--all", "--prune"])
}

fn pull(path: &str) -> Result<String, String> {
    let s = sync_status(&open(path)?)?;
    if s.branch.is_none() {
        return Err("Check out a branch before pulling".into());
    }
    if s.upstream.is_none() {
        return Err("The current branch has no upstream to pull from. Push it first.".into());
    }
    // Fast-forward only: never creates a surprise merge commit or leaves a conflicted tree.
    run_git(path, &["pull", "--ff-only"])
}

fn push(path: &str) -> Result<String, String> {
    let repo = open(path)?;
    let s = sync_status(&repo)?;
    if s.branch.is_none() {
        return Err("Cannot push a detached HEAD. Check out a branch first.".into());
    }
    if s.upstream.is_some() {
        return run_git(path, &["push"]);
    }
    // First push of a new branch: publish it to the target remote and start tracking it.
    let remote = crate::sidebar::remotes::target_remote(&repo).ok_or("No remotes configured for this repository")?;
    run_git(path, &["push", "--set-upstream", &remote, "HEAD"])
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BriefCommit {
    pub short_id: String,
    pub summary: String,
}

/// The two sides of a diverged branch: what only you have, and what only the upstream has.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Divergence {
    pub branch: String,
    pub upstream: String,
    /// Newest first, at most LIST_LIMIT entries; the totals count everything.
    pub ahead: Vec<BriefCommit>,
    pub behind: Vec<BriefCommit>,
    pub ahead_total: usize,
    pub behind_total: usize,
}

const LIST_LIMIT: usize = 30;

/// Commits reachable from `include` but not from `exclude`, newest first.
fn side_commits(repo: &Repository, include: git2::Oid, exclude: git2::Oid) -> Result<Vec<BriefCommit>, String> {
    let mut walk = repo.revwalk().map_err(err)?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME).map_err(err)?;
    walk.push(include).map_err(err)?;
    walk.hide(exclude).map_err(err)?;
    let mut out = Vec::new();
    for oid in walk.take(LIST_LIMIT) {
        let oid = oid.map_err(err)?;
        let commit = repo.find_commit(oid).map_err(err)?;
        out.push(BriefCommit {
            short_id: oid.to_string()[..7].to_string(),
            summary: commit.summary().ok().flatten().unwrap_or("").to_string(),
        });
    }
    Ok(out)
}

fn divergence(path: &str) -> Result<Divergence, String> {
    let repo = open(path)?;
    let s = sync_status(&repo)?;
    let (Some(branch), Some(upstream)) = (s.branch, s.upstream) else {
        return Err("The current branch has no upstream to compare with".into());
    };
    let local = repo.head().and_then(|h| h.peel_to_commit()).map_err(err)?.id();
    let remote = repo
        .find_branch(&branch, BranchType::Local)
        .and_then(|b| b.upstream())
        .map_err(err)?
        .get()
        .target()
        .ok_or("The upstream branch has no commits")?;
    let (ahead_total, behind_total) = repo.graph_ahead_behind(local, remote).map_err(err)?;
    Ok(Divergence {
        ahead: side_commits(&repo, local, remote)?,
        behind: side_commits(&repo, remote, local)?,
        ahead_total,
        behind_total,
        branch,
        upstream,
    })
}

/// Pulls when fast-forwarding is not possible: "merge" adds a merge commit, "rebase" replays the
/// local commits on top of the upstream. Uncommitted changes are set aside and restored
/// (--autostash). If the result has conflicts there is no way to resolve them in the app yet,
/// so the operation is cancelled and the repository is left exactly as it was.
fn pull_with(path: &str, mode: &str) -> Result<String, String> {
    let s = sync_status(&open(path)?)?;
    if s.branch.is_none() {
        return Err("Check out a branch before pulling".into());
    }
    if s.upstream.is_none() {
        return Err("The current branch has no upstream to pull from. Push it first.".into());
    }
    let (args, retry_hint): (&[&str], &str) = match mode {
        "merge" => (&["pull", "--no-rebase", "--no-edit", "--autostash"], "git pull --no-rebase"),
        "rebase" => (&["pull", "--rebase", "--autostash"], "git pull --rebase"),
        other => return Err(format!("Unknown pull mode \"{other}\"")),
    };
    run_git(path, args).map_err(|original| cancel_unfinished(path, retry_hint, original))
}

/// After a failed pull: if git stopped halfway (merge or rebase with conflicts), cancel it and say
/// which files conflicted. Otherwise return git's own message unchanged.
pub(crate) fn cancel_unfinished(path: &str, retry_hint: &str, original: String) -> String {
    let Ok(repo) = open(path) else { return original };
    let (abort, what): (&[&str], &str) = match repo.state() {
        RepositoryState::Merge => (&["merge", "--abort"], "merge"),
        RepositoryState::Rebase | RepositoryState::RebaseInteractive | RepositoryState::RebaseMerge => {
            (&["rebase", "--abort"], "rebase")
        }
        _ => return original,
    };
    let files = run_git(path, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default();
    let files: Vec<&str> = files.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let which = if files.is_empty() { String::new() } else { format!(" in: {}", files.join(", ")) };
    match run_git(path, abort) {
        Ok(_) => format!(
            "The {what} has conflicts{which}.\n\nIt was cancelled and nothing was changed. Resolve it in the terminal with \"{retry_hint}\"."
        ),
        Err(e) => format!("{original}\n\nThe {what} could not be cancelled automatically: {e}"),
    }
}

/// Result of a background fetch. It never fails as a command: the frontend decides what to do
/// from `status`: "ok", "none" (no remotes), "auth" (credentials needed), "offline" or "error".
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutoFetchOutcome {
    pub status: &'static str,
    pub message: String,
}

/// Sorts a git error message into something the auto-fetch scheduler can react to.
fn classify_fetch_error(message: &str) -> &'static str {
    let m = message.to_lowercase();
    let any = |needles: &[&str]| needles.iter().any(|n| m.contains(n));
    if any(&[
        "authentication failed",
        "could not read username",
        "could not read password",
        "terminal prompts disabled",
        "permission denied (publickey",
        "invalid username or password",
        "requested url returned error: 401",
        "requested url returned error: 403",
        "repository not found",
    ]) {
        "auth"
    } else if any(&[
        "could not resolve host",
        "could not resolve hostname",
        "unable to access",
        "connection timed out",
        "connection refused",
        "connection reset",
        "network is unreachable",
        "failed to connect",
        "could not connect",
        "operation timed out",
        "timed out after",
        "temporary failure in name resolution",
    ]) {
        "offline"
    } else {
        "error"
    }
}

/// Quiet background fetch: no pruning, and no write to FETCH_HEAD (so the repo watcher doesn't
/// see a change when nothing arrived) or background maintenance. A hung connection is cut off.
fn auto_fetch(path: &str) -> AutoFetchOutcome {
    let outcome = |status, message: String| AutoFetchOutcome { status, message };
    match open(path).and_then(|r| sync_status(&r)) {
        Ok(s) if !s.has_remote => return outcome("none", String::new()),
        Err(e) => return outcome("error", e),
        Ok(_) => {}
    }

    let timeout = Some(Duration::from_secs(90));
    let mut result = run_git_with(path, &["fetch", "--all", "--no-write-fetch-head", "--no-auto-gc"], timeout);
    // Older git versions don't know those flags: fall back to a plain fetch.
    if matches!(&result, Err(e) if e.to_lowercase().contains("unknown option")) {
        result = run_git_with(path, &["fetch", "--all"], timeout);
    }
    match result {
        Ok(_) => outcome("ok", String::new()),
        Err(e) => outcome(classify_fetch_error(&e), e),
    }
}

/// Overwrites the upstream with the local branch, e.g. after an amend or rename. Uses
/// --force-with-lease, so it refuses if the remote branch moved since the last fetch (somebody
/// else's commits are never thrown away unseen).
fn force_push(path: &str) -> Result<String, String> {
    let s = sync_status(&open(path)?)?;
    if s.branch.is_none() {
        return Err("Cannot push a detached HEAD. Check out a branch first.".into());
    }
    if s.upstream.is_none() {
        return Err("This branch has not been pushed yet, so there is nothing to overwrite. Use Push instead.".into());
    }
    run_git(path, &["push", "--force-with-lease"])
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_sync_status(path: String) -> Result<SyncStatus, String> {
    blocking(move || sync_status(&open(&path)?)).await
}

#[tauri::command]
pub async fn git_fetch(path: String) -> Result<String, String> {
    blocking(move || fetch(&path)).await
}

#[tauri::command]
pub async fn git_pull(path: String) -> Result<String, String> {
    crate::undo::recorded(&path.clone(), "Pull", crate::undo::Kind::Switch, || blocking(move || pull(&path))).await
}

#[tauri::command]
pub async fn git_push(path: String) -> Result<String, String> {
    blocking(move || push(&path)).await
}

/// What a pull would be combining when the branches have diverged.
#[tauri::command]
pub async fn get_divergence(path: String) -> Result<Divergence, String> {
    blocking(move || divergence(&path)).await
}

/// Pull by merging ("merge") or rebasing ("rebase"), for branches that can't fast-forward.
#[tauri::command]
pub async fn git_pull_with(path: String, mode: String) -> Result<String, String> {
    let label = format!("Pull ({mode})");
    crate::undo::recorded(&path.clone(), label, crate::undo::Kind::Switch, || blocking(move || pull_with(&path, &mode)))
        .await
}

#[tauri::command]
pub async fn git_auto_fetch(path: String) -> Result<AutoFetchOutcome, String> {
    blocking(move || Ok(auto_fetch(&path))).await
}

#[tauri::command]
pub async fn git_force_push(path: String) -> Result<String, String> {
    blocking(move || force_push(&path)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gc-sync-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        run_git(dir.to_str().unwrap(), args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    }

    fn commit_file(dir: &Path, file: &str, msg: &str) {
        fs::write(dir.join(file), msg).unwrap();
        git(dir, &["add", "-A"]);
        git(dir, &["-c", "user.name=T", "-c", "user.email=t@example.com", "commit", "-q", "-m", msg]);
    }

    #[test]
    fn fetch_pull_push_roundtrip() {
        let base = tmp("rt");
        let origin = base.join("origin.git");
        let a = base.join("a");
        let b = base.join("b");
        fs::create_dir_all(&a).unwrap();
        git(&base, &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
        git(&a, &["init", "-q", "-b", "main"]);
        git(&a, &["remote", "add", "origin", origin.to_str().unwrap()]);

        // No remote-less errors, and a repo without commits has no branch status.
        let (ap, bp) = (a.to_str().unwrap(), b.to_str().unwrap());
        assert!(pull(ap).is_err());

        commit_file(&a, "f.txt", "one");
        let s = sync_status(&open(ap).unwrap()).unwrap();
        assert_eq!((s.upstream.clone(), s.has_remote), (None, true));

        // First push publishes the branch and sets the upstream.
        push(ap).unwrap();
        let s = sync_status(&open(ap).unwrap()).unwrap();
        assert_eq!(s.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.ahead, s.behind), (0, 0));

        git(&base, &["clone", "-q", origin.to_str().unwrap(), bp]);

        // A pushes a second commit; B only learns about it after fetching.
        commit_file(&a, "f.txt", "two");
        push(ap).unwrap();
        let s = sync_status(&open(bp).unwrap()).unwrap();
        assert_eq!(s.behind, 0);
        fetch(bp).unwrap();
        let s = sync_status(&open(bp).unwrap()).unwrap();
        assert_eq!((s.ahead, s.behind), (0, 1));

        pull(bp).unwrap();
        let s = sync_status(&open(bp).unwrap()).unwrap();
        assert_eq!((s.ahead, s.behind), (0, 0));

        // B commits locally -> ahead 1 -> push brings origin level.
        commit_file(&b, "g.txt", "three");
        assert_eq!(sync_status(&open(bp).unwrap()).unwrap().ahead, 1);
        push(bp).unwrap();
        assert_eq!(sync_status(&open(bp).unwrap()).unwrap().ahead, 0);

        // A has diverged by committing without pulling: ff-only pull must refuse.
        commit_file(&a, "h.txt", "four");
        fetch(ap).unwrap();
        assert!(pull(ap).is_err());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn force_push_overwrites_but_respects_the_lease() {
        let base = tmp("force");
        let origin = base.join("origin.git");
        let (a, b) = (base.join("a"), base.join("b"));
        fs::create_dir_all(&a).unwrap();
        git(&base, &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
        git(&a, &["init", "-q", "-b", "main"]);
        git(&a, &["remote", "add", "origin", origin.to_str().unwrap()]);
        let (ap, bp) = (a.to_str().unwrap(), b.to_str().unwrap());
        let amend = |dir: &Path, msg: &str| {
            git(dir, &["-c", "user.name=T", "-c", "user.email=t@example.com", "commit", "-q", "--amend", "-m", msg]);
        };
        let head = |dir: &Path| git(dir, &["rev-parse", "HEAD"]);
        let remote_main = || git(&base, &["--git-dir", origin.to_str().unwrap(), "rev-parse", "main"]);

        // Nothing to force before the branch has been published.
        commit_file(&a, "f.txt", "one");
        assert!(force_push(ap).unwrap_err().contains("not been pushed"));
        push(ap).unwrap();
        git(&base, &["clone", "-q", origin.to_str().unwrap(), bp]);

        // After an amend a normal push is rejected, a force push goes through.
        amend(&a, "one, amended");
        assert!(push(ap).is_err());
        force_push(ap).unwrap();
        assert_eq!(remote_main(), head(&a));

        // Somebody else pushes; our next force push must not clobber it unseen.
        git(&b, &["fetch", "-q"]);
        git(&b, &["reset", "-q", "--hard", "origin/main"]);
        commit_file(&b, "g.txt", "theirs");
        push(bp).unwrap();
        let theirs = remote_main();
        amend(&a, "one, amended again");
        assert!(force_push(ap).is_err());
        assert_eq!(remote_main(), theirs);

        // Once we have fetched and seen their commit, the overwrite is allowed.
        fetch(ap).unwrap();
        force_push(ap).unwrap();
        assert_eq!(remote_main(), head(&a));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn fetch_errors_are_classified() {
        for m in [
            "fatal: Authentication failed for 'https://github.com/x/y.git/'",
            "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
            "git@github.com: Permission denied (publickey).",
            "remote: Repository not found.",
        ] {
            assert_eq!(classify_fetch_error(m), "auth", "{m}");
        }
        for m in [
            "fatal: unable to access 'https://x/': Could not resolve host: x",
            "ssh: connect to host x port 22: Connection timed out",
            "git fetch timed out after 90s",
        ] {
            assert_eq!(classify_fetch_error(m), "offline", "{m}");
        }
        assert_eq!(classify_fetch_error("fatal: something else entirely"), "error");
    }

    #[test]
    fn run_git_with_kills_slow_commands() {
        let dir = tmp("timeout");
        git(&dir, &["init", "-q"]);
        // A fast command finishes well inside the limit.
        assert!(run_git_with(dir.to_str().unwrap(), &["status"], Some(Duration::from_secs(30))).is_ok());
        // An alias that sleeps is cut off after the timeout.
        #[cfg(windows)]
        let slow = "alias.slow=!powershell -NoProfile -Command Start-Sleep -Seconds 20";
        #[cfg(not(windows))]
        let slow = "alias.slow=!sleep 20";
        let started = Instant::now();
        let e =
            run_git_with(dir.to_str().unwrap(), &["-c", slow, "slow"], Some(Duration::from_millis(500))).unwrap_err();
        assert!(e.contains("timed out"), "{e}");
        assert!(started.elapsed() < Duration::from_secs(15));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_fetch_updates_refs_quietly() {
        let base = tmp("autofetch");
        let origin = base.join("origin.git");
        let (a, b) = (base.join("a"), base.join("b"));
        fs::create_dir_all(&a).unwrap();
        git(&base, &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
        git(&a, &["init", "-q", "-b", "main"]);
        // No remote yet: nothing to do.
        assert_eq!(auto_fetch(a.to_str().unwrap()).status, "none");
        git(&a, &["remote", "add", "origin", origin.to_str().unwrap()]);
        commit_file(&a, "f.txt", "one");
        push(a.to_str().unwrap()).unwrap();
        git(&base, &["clone", "-q", origin.to_str().unwrap(), b.to_str().unwrap()]);
        let bp = b.to_str().unwrap();

        commit_file(&a, "f.txt", "two");
        push(a.to_str().unwrap()).unwrap();
        assert_eq!(sync_status(&open(bp).unwrap()).unwrap().behind, 0);

        let outcome = auto_fetch(bp);
        assert_eq!(outcome.status, "ok", "{}", outcome.message);
        assert_eq!(sync_status(&open(bp).unwrap()).unwrap().behind, 1);
        // Quiet: FETCH_HEAD is not written, so the repo watcher stays silent when nothing changed.
        assert!(!b.join(".git").join("FETCH_HEAD").exists());

        // An unreachable remote is reported, not thrown.
        git(&b, &["remote", "set-url", "origin", base.join("missing.git").to_str().unwrap()]);
        assert_ne!(auto_fetch(bp).status, "ok");

        let _ = fs::remove_dir_all(&base);
    }

    /// Two clones of one origin. Both have configured identities so merges and rebases can commit.
    fn clone_pair(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let base = tmp(name);
        let origin = base.join("origin.git");
        let (a, b) = (base.join("a"), base.join("b"));
        fs::create_dir_all(&a).unwrap();
        git(&base, &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
        git(&a, &["init", "-q", "-b", "main"]);
        git(&a, &["remote", "add", "origin", origin.to_str().unwrap()]);
        git(&a, &["config", "user.name", "T"]);
        git(&a, &["config", "user.email", "t@example.com"]);
        commit_file(&a, "base.txt", "base");
        push(a.to_str().unwrap()).unwrap();
        git(&base, &["clone", "-q", origin.to_str().unwrap(), b.to_str().unwrap()]);
        git(&b, &["config", "user.name", "T"]);
        git(&b, &["config", "user.email", "t@example.com"]);
        (base, a, b)
    }

    /// a and b each commit; b pushes; a fetches: a is now one ahead and one behind.
    fn diverge(a: &Path, b: &Path, a_file: &str, b_file: &str) {
        commit_file(a, a_file, "mine commit");
        commit_file(b, b_file, "theirs commit");
        push(b.to_str().unwrap()).unwrap();
        fetch(a.to_str().unwrap()).unwrap();
    }

    fn parents_of_head(dir: &Path) -> usize {
        git(dir, &["rev-list", "--parents", "-n", "1", "HEAD"]).split_whitespace().count() - 1
    }

    #[test]
    fn diverged_pull_by_merge_or_rebase() {
        // --- merge ---
        let (base, a, b) = clone_pair("pull-merge");
        let ap = a.to_str().unwrap();
        diverge(&a, &b, "mine.txt", "theirs.txt");

        let d = divergence(ap).unwrap();
        assert_eq!((d.branch.as_str(), d.upstream.as_str()), ("main", "origin/main"));
        assert_eq!((d.ahead_total, d.behind_total), (1, 1));
        assert_eq!(d.ahead, [BriefCommit { short_id: d.ahead[0].short_id.clone(), summary: "mine commit".into() }]);
        assert_eq!(d.behind[0].summary, "theirs commit");
        // The plain fast-forward-only pull refuses, with a message the UI can recognise.
        let e = pull(ap).unwrap_err().to_lowercase();
        assert!(e.contains("fast-forward") || e.contains("diverging"), "{e}");

        // An unrelated uncommitted edit is set aside and restored around the merge.
        fs::write(a.join("base.txt"), "base, edited locally").unwrap();
        pull_with(ap, "merge").unwrap();
        assert_eq!(parents_of_head(&a), 2);
        assert!(a.join("mine.txt").exists() && a.join("theirs.txt").exists());
        assert_eq!(fs::read_to_string(a.join("base.txt")).unwrap(), "base, edited locally");
        let s = sync_status(&open(ap).unwrap()).unwrap();
        assert_eq!((s.ahead, s.behind), (2, 0));
        assert!(pull_with(ap, "octopus").unwrap_err().contains("Unknown pull mode"));
        let _ = fs::remove_dir_all(&base);

        // --- rebase ---
        let (base, a, b) = clone_pair("pull-rebase");
        let ap = a.to_str().unwrap();
        diverge(&a, &b, "mine.txt", "theirs.txt");
        let before = git(&a, &["rev-parse", "HEAD"]);
        pull_with(ap, "rebase").unwrap();
        assert_eq!(parents_of_head(&a), 1); // linear history
        assert_ne!(git(&a, &["rev-parse", "HEAD"]), before); // our commit was replayed
        assert!(a.join("mine.txt").exists() && a.join("theirs.txt").exists());
        let s = sync_status(&open(ap).unwrap()).unwrap();
        assert_eq!((s.ahead, s.behind), (1, 0));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn conflicting_pulls_are_cancelled_and_leave_no_trace() {
        let (base, a, b) = clone_pair("pull-conflict");
        let ap = a.to_str().unwrap();
        // Both sides rewrite the same file differently.
        diverge(&a, &b, "base.txt", "base.txt");
        let before = git(&a, &["rev-parse", "HEAD"]);

        for mode in ["merge", "rebase"] {
            let e = pull_with(ap, mode).unwrap_err();
            assert!(e.contains("conflicts in: base.txt") && e.contains("cancelled"), "{mode}: {e}");
            // Nothing half-done is left behind.
            assert_eq!(open(ap).unwrap().state(), RepositoryState::Clean, "{mode}");
            assert_eq!(git(&a, &["rev-parse", "HEAD"]), before, "{mode}");
            assert_eq!(fs::read_to_string(a.join("base.txt")).unwrap(), "mine commit", "{mode}");
            assert!(!a.join(".git").join("rebase-merge").exists() && !a.join(".git").join("MERGE_HEAD").exists());
        }
        let _ = fs::remove_dir_all(&base);
    }
}
