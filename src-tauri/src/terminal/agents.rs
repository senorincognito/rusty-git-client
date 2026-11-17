use std::collections::HashMap;
use std::path::PathBuf;

/// The program of an agent command ("claude --flag" is "claude"). None when it has characters a shell could read as
/// something else, which are not looked up (and not reported missing).
fn program(command: &str) -> Option<&str> {
    let prog = command.split_whitespace().next()?;
    prog.chars().all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c)).then_some(prog)
}

/// Whether `prog` is a file in one of `dirs`, as is or with one of `exts` appended (".exe", ".cmd", ... on Windows).
fn exists_in(prog: &str, dirs: &[PathBuf], exts: &[String]) -> bool {
    dirs.iter().any(|dir| dir.join(prog).is_file() || exts.iter().any(|ext| dir.join(format!("{prog}{ext}")).is_file()))
}

#[cfg(windows)]
fn on_path(prog: &str) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    let exts: Vec<String> = exts.split(';').filter(|e| !e.is_empty()).map(str::to_string).collect();
    exists_in(prog, &dirs, &exts)
}

/// Asks the login shell, like the terminal panel's shell: an app started from the Dock has not read the user's profile.
#[cfg(not(windows))]
fn on_path(prog: &str) -> bool {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    std::process::Command::new(shell)
        .args(["-lc", &format!("command -v -- {prog}")])
        .stdin(std::process::Stdio::null())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(true)
}

/// For each command: whether its program can be found. A command that can't be checked counts as found. The shell may
/// still know something this misses (an alias, a function), so the UI only warns.
fn check(commands: &[String], on_path: impl Fn(&str) -> bool) -> HashMap<String, bool> {
    commands.iter().map(|c| (c.clone(), program(c).map(&on_path).unwrap_or(true))).collect()
}

#[tauri::command]
pub async fn check_agent_commands(commands: Vec<String>) -> Result<HashMap<String, bool>, String> {
    tauri::async_runtime::spawn_blocking(move || check(&commands, on_path)).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_programs_with_or_without_an_extension() {
        let dir = std::env::temp_dir().join(format!("gc-agents-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("fake-agent.cmd"), "").unwrap();
        std::fs::write(dir.join("plain"), "").unwrap();
        let dirs = vec![PathBuf::from("does-not-exist"), dir.clone()];
        let exts = vec![".exe".to_string(), ".cmd".to_string()];

        assert!(exists_in("fake-agent", &dirs, &exts));
        assert!(exists_in("fake-agent.cmd", &dirs, &exts));
        assert!(exists_in("plain", &dirs, &[]));
        assert!(!exists_in("fake-agent", &dirs, &[]));
        assert!(!exists_in("missing", &dirs, &exts));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_plain_program_names_are_looked_up() {
        assert_eq!(program("claude --resume"), Some("claude"));
        assert_eq!(program("  aider  "), Some("aider"));
        assert_eq!(program("ollama run llama3"), Some("ollama"));
        assert_eq!(program("a;b"), None);
        assert_eq!(program("$(evil)"), None);
        assert_eq!(program(""), None);

        let found = check(&["claude".into(), "x;y".into(), "nope".into()], |p| p == "claude");
        assert!(found["claude"]);
        assert!(found["x;y"], "an unchecked command is not reported missing");
        assert!(!found["nope"]);
    }
}
