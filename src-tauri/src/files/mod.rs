pub mod blame;

use std::collections::BTreeSet;
use std::path::{Component, Path};
use std::time::Duration;

use serde::Serialize;

/// Larger files are not read into the viewer.
const MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Every file of the working directory that git knows or would show: tracked ones plus untracked ones that are not
/// ignored, as `/`-separated paths relative to the repository, sorted. Tracked files deleted on disk are left out
/// (there is nothing to open), and so are submodules (directories on disk).
fn list_files(repo_path: &str) -> Result<Vec<String>, String> {
    let out = crate::toolbar::sync::run_git_with(
        repo_path,
        &["-c", "core.quotepath=false", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        Some(Duration::from_secs(60)),
    )?;
    let root = Path::new(repo_path);
    // A set: a file with merge conflicts is listed once per stage.
    let names: BTreeSet<&str> = out.split('\0').filter(|n| !n.is_empty()).collect();
    Ok(names
        .into_iter()
        .filter(|n| std::fs::metadata(root.join(n)).is_ok_and(|m| m.is_file()))
        .map(str::to_string)
        .collect())
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    pub size: u64,
    /// "text", "binary" or "tooLarge"; `content` is only there for text.
    pub kind: &'static str,
    pub content: Option<String>,
}

/// `file` as a path relative to the repository: no `..`, no root, nothing in git's own directory.
fn relative(file: &str) -> Result<&Path, String> {
    let rel = Path::new(file);
    let mut parts = rel.components();
    let normal = !file.is_empty() && parts.clone().all(|c| matches!(c, Component::Normal(_)));
    if !normal {
        return Err(format!("\"{file}\" is not a path inside the repository"));
    }
    if matches!(parts.next(), Some(Component::Normal(n)) if n.eq_ignore_ascii_case(".git")) {
        return Err(format!("\"{file}\" is in git's own directory"));
    }
    Ok(rel)
}

/// Creates (`create_only`) or overwrites the file `file`, making missing folders, and never leaves the repository.
fn write_file(repo_path: &str, file: &str, content: &str, create_only: bool) -> Result<(), String> {
    let rel = relative(file)?;
    let root = Path::new(repo_path).canonicalize().map_err(|e| e.to_string())?;
    let full = root.join(rel);
    if let Ok(meta) = std::fs::symlink_metadata(&full) {
        if create_only {
            return Err(format!("\"{file}\" already exists"));
        }
        if meta.is_dir() {
            return Err(format!("\"{file}\" is a folder"));
        }
        // Writing through a symlink is fine only while it stays inside the repository.
        if !full.canonicalize().map_err(|e| e.to_string())?.starts_with(&root) {
            return Err(format!("\"{file}\" is not inside the repository"));
        }
    }
    let parent = full.parent().unwrap_or(&root);
    // The nearest folder that exists decides where new ones would end up (a symlinked folder may lead out).
    let existing = parent.ancestors().find(|a| a.exists()).unwrap_or(&root);
    if !existing.canonicalize().map_err(|e| e.to_string())?.starts_with(&root) {
        return Err(format!("\"{file}\" is not inside the repository"));
    }
    std::fs::create_dir_all(parent).map_err(|e| format!("{file}: {e}"))?;
    std::fs::write(&full, content).map_err(|e| format!("{file}: {e}"))
}

/// Deletes the file `file` from the working directory (not folders).
fn delete_file(repo_path: &str, file: &str) -> Result<(), String> {
    let rel = relative(file)?;
    let root = Path::new(repo_path).canonicalize().map_err(|e| e.to_string())?;
    let full = root.join(rel);
    let meta = std::fs::symlink_metadata(&full).map_err(|e| format!("{file}: {e}"))?;
    if meta.is_dir() {
        return Err(format!("\"{file}\" is a folder"));
    }
    // The file itself may be a symlink (only the link goes), but the folder it sits in must be inside.
    let parent = full.parent().unwrap_or(&root).canonicalize().map_err(|e| e.to_string())?;
    if !parent.starts_with(&root) {
        return Err(format!("\"{file}\" is not inside the repository"));
    }
    std::fs::remove_file(&full).map_err(|e| format!("{file}: {e}"))
}

/// The working-directory file at `file` (relative to the repository). Refuses anything that resolves outside it.
fn read_file(repo_path: &str, file: &str) -> Result<FileContent, String> {
    let rel = relative(file)?;
    let root = Path::new(repo_path).canonicalize().map_err(|e| e.to_string())?;
    let full = root.join(rel).canonicalize().map_err(|e| format!("{file}: {e}"))?;
    // A symlink inside the repository may point anywhere.
    if !full.starts_with(&root) {
        return Err(format!("\"{file}\" is not inside the repository"));
    }
    let size = std::fs::metadata(&full).map_err(|e| e.to_string())?.len();
    if size > MAX_BYTES {
        return Ok(FileContent { size, kind: "tooLarge", content: None });
    }
    let bytes = std::fs::read(&full).map_err(|e| e.to_string())?;
    if bytes[..bytes.len().min(8000)].contains(&0) {
        return Ok(FileContent { size, kind: "binary", content: None });
    }
    let content = String::from_utf8_lossy(&bytes).into_owned();
    Ok(FileContent { size, kind: "text", content: Some(content) })
}

/// Every file in the working directory, for the Files tab.
#[tauri::command]
pub async fn get_files(path: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || list_files(&path)).await.map_err(|e| e.to_string())?
}

/// The content of one working-directory file, for the Files tab's viewer.
#[tauri::command]
pub async fn get_file_content(path: String, file: String) -> Result<FileContent, String> {
    tauri::async_runtime::spawn_blocking(move || read_file(&path, &file)).await.map_err(|e| e.to_string())?
}

/// A new, empty-or-filled file (refuses an existing name), for the Files tab.
#[tauri::command]
pub async fn create_file_cmd(path: String, file: String, content: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || write_file(&path, &file, &content, true))
        .await
        .map_err(|e| e.to_string())?
}

/// Overwrites a file with the edited text.
#[tauri::command]
pub async fn save_file_cmd(path: String, file: String, content: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || write_file(&path, &file, &content, false))
        .await
        .map_err(|e| e.to_string())?
}

/// Deletes a file from the working directory.
#[tauri::command]
pub async fn delete_file_cmd(path: String, file: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || delete_file(&path, &file)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{commit_file, new_repo};

    #[test]
    fn lists_tracked_and_untracked_files_but_not_ignored_or_deleted_ones() {
        let (dir, repo) = new_repo("files-list");
        commit_file(&repo, &dir, ".gitignore", "ignored.txt\n", "ignore");
        commit_file(&repo, &dir, "a.txt", "a", "a");
        commit_file(&repo, &dir, "gone.txt", "g", "g");
        std::fs::create_dir_all(dir.join("src/deep")).unwrap();
        std::fs::write(dir.join("src/deep/b.txt"), "b").unwrap();
        std::fs::write(dir.join("ignored.txt"), "i").unwrap();
        std::fs::remove_file(dir.join("gone.txt")).unwrap();
        let files = list_files(dir.to_str().unwrap()).unwrap();
        assert_eq!(files, [".gitignore", "a.txt", "src/deep/b.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_text_binary_and_large_files() {
        let (dir, repo) = new_repo("files-read");
        commit_file(&repo, &dir, "t.txt", "héllo\n", "t");
        std::fs::write(dir.join("b.bin"), [1u8, 0, 2]).unwrap();
        std::fs::write(dir.join("big.txt"), vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        let p = dir.to_str().unwrap();
        assert_eq!(
            read_file(p, "t.txt").unwrap(),
            FileContent { size: 7, kind: "text", content: Some("héllo\n".into()) }
        );
        assert_eq!(read_file(p, "b.bin").unwrap().kind, "binary");
        let big = read_file(p, "big.txt").unwrap();
        assert_eq!((big.kind, big.content), ("tooLarge", None));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_paths_outside_the_repository() {
        let (dir, _repo) = new_repo("files-escape");
        let p = dir.to_str().unwrap();
        assert!(read_file(p, "../secret.txt").unwrap_err().contains("not a path inside"));
        assert!(read_file(p, "/etc/passwd").is_err());
        assert!(read_file(p, "missing.txt").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn creates_saves_and_deletes_files() {
        let (dir, _repo) = new_repo("files-write");
        let p = dir.to_str().unwrap();
        write_file(p, "new/deep/n.txt", "one", true).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("new/deep/n.txt")).unwrap(), "one");
        assert!(write_file(p, "new/deep/n.txt", "x", true).unwrap_err().contains("already exists"));
        write_file(p, "new/deep/n.txt", "two", false).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("new/deep/n.txt")).unwrap(), "two");
        assert!(write_file(p, "new", "x", false).unwrap_err().contains("folder"));
        assert!(delete_file(p, "new").unwrap_err().contains("folder"));
        delete_file(p, "new/deep/n.txt").unwrap();
        assert!(!dir.join("new/deep/n.txt").exists());
        assert!(delete_file(p, "new/deep/n.txt").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_refuse_paths_outside_or_in_git_dir() {
        let (dir, _repo) = new_repo("files-write-escape");
        let p = dir.to_str().unwrap();
        for bad in ["../x.txt", "a/../../x.txt", ".git/config", ".GIT/hooks/x", "", "/abs.txt"] {
            assert!(write_file(p, bad, "x", false).is_err(), "{bad}");
            assert!(delete_file(p, bad).is_err(), "{bad}");
        }
        assert!(!dir.parent().unwrap().join("x.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
