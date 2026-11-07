use git2::{Oid, Reference, Repository};
use serde::Serialize;

fn err(e: git2::Error) -> String {
    e.message().to_string()
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TagInfo {
    pub name: String,
    /// The commit the tag points at (an annotated tag is followed to it).
    pub id: String,
    pub short_id: String,
    pub annotated: bool,
    /// The annotation's text of an annotated tag.
    pub message: Option<String>,
    /// Unix seconds: when an annotated tag was made, else the commit's time.
    pub time: i64,
}

/// Every tag of the repository, newest first. Tags that don't lead to a commit are left out.
fn list(path: &str) -> Result<Vec<TagInfo>, String> {
    let repo = Repository::discover(path).map_err(err)?;
    let mut out = Vec::new();
    for reference in repo.references_glob("refs/tags/*").map_err(err)? {
        let reference = reference.map_err(err)?;
        let Some(name) = reference.name().ok().and_then(|n| n.strip_prefix("refs/tags/")) else { continue };
        let Ok(commit) = reference.peel_to_commit() else { continue };
        let annotation = reference.target().and_then(|oid| repo.find_tag(oid).ok());
        let (message, time) = match &annotation {
            Some(tag) => (
                tag.message().ok().flatten().map(|m| m.trim().to_string()).filter(|m| !m.is_empty()),
                tag.tagger().map_or(commit.time().seconds(), |s| s.when().seconds()),
            ),
            None => (None, commit.time().seconds()),
        };
        out.push(TagInfo {
            name: name.to_string(),
            id: commit.id().to_string(),
            short_id: commit.id().to_string()[..7].to_string(),
            annotated: annotation.is_some(),
            message,
            time,
        });
    }
    out.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

/// Tags commit `id`: a lightweight tag, or an annotated one (tagger = you) when `message` has text. An existing name is
/// refused rather than moved.
fn create(path: &str, id: &str, name: &str, message: Option<&str>) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a name for the tag".into());
    }
    if !Reference::is_valid_name(&format!("refs/tags/{name}")) {
        return Err(format!("\"{name}\" is not a valid tag name"));
    }
    let oid = Oid::from_str(id).map_err(|_| format!("\"{id}\" is not a commit id"))?;
    let repo = Repository::discover(path).map_err(err)?;
    if repo.find_reference(&format!("refs/tags/{name}")).is_ok() {
        return Err(format!("A tag named \"{name}\" already exists"));
    }
    if crate::sidebar::stash::stash_index_of(&repo, oid).is_some() {
        return Err("A stash can't be tagged".into());
    }
    let commit = repo.find_commit(oid).map_err(|_| format!("Commit {id} was not found"))?;
    match message.map(str::trim).filter(|m| !m.is_empty()) {
        Some(message) => {
            let tagger = repo
                .signature()
                .map_err(|_| "Set user.name and user.email in git to create an annotated tag".to_string())?;
            repo.tag(name, commit.as_object(), &tagger, message, false).map_err(err)?;
        }
        None => {
            repo.tag_lightweight(name, commit.as_object(), false).map_err(err)?;
        }
    }
    Ok(())
}

/// Deletes the local tag `name`. The server's copy, if the tag was ever pushed, is left alone.
fn delete(path: &str, name: &str) -> Result<(), String> {
    let repo = Repository::discover(path).map_err(err)?;
    if repo.find_reference(&format!("refs/tags/{name}")).is_err() {
        return Err(format!("There is no tag named \"{name}\""));
    }
    repo.tag_delete(name).map_err(err)
}

#[tauri::command]
pub async fn get_tags(path: String) -> Result<Vec<TagInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || list(&path)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn create_tag_cmd(path: String, id: String, name: String, message: Option<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || create(&path, &id, &name, message.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_tag_cmd(path: String, name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || delete(&path, &name)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::test_support::{commit_file, new_repo};

    #[test]
    fn lists_tags_with_their_commits_and_annotations() {
        let (dir, repo) = new_repo("tags-list");
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "T").unwrap();
        cfg.set_str("user.email", "t@example.com").unwrap();
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        let b = commit_file(&repo, &dir, "f.txt", "b", "B");
        let p = dir.to_str().unwrap();
        assert!(list(p).unwrap().is_empty());

        create(p, &a.to_string(), "v1", None).unwrap();
        create(
            p,
            &b.to_string(),
            "v2",
            Some(
                "Second release

more",
            ),
        )
        .unwrap();
        let tags = list(p).unwrap();
        assert_eq!(tags.len(), 2);
        let v1 = tags.iter().find(|t| t.name == "v1").unwrap();
        assert_eq!((v1.id.as_str(), v1.annotated, v1.message.as_deref()), (a.to_string().as_str(), false, None));
        let v2 = tags.iter().find(|t| t.name == "v2").unwrap();
        // An annotated tag is followed to its commit, not listed under the tag object's id.
        assert_eq!((v2.id.as_str(), v2.annotated), (b.to_string().as_str(), true));
        assert!(v2.message.as_deref().unwrap().starts_with("Second release"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn creates_and_deletes_tags() {
        let (dir, repo) = new_repo("tags");
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "T").unwrap();
        cfg.set_str("user.email", "t@example.com").unwrap();
        let a = commit_file(&repo, &dir, "f.txt", "a", "A");
        let b = commit_file(&repo, &dir, "f.txt", "b", "B");
        let p = dir.to_str().unwrap();

        // Lightweight: the ref points straight at the commit.
        create(p, &a.to_string(), " v1 ", None).unwrap();
        assert_eq!(repo.find_reference("refs/tags/v1").unwrap().target(), Some(a));
        // Annotated: a tag object that peels to the commit; a blank message means lightweight.
        create(p, &b.to_string(), "release/2", Some("Second release")).unwrap();
        let r = repo.find_reference("refs/tags/release/2").unwrap();
        assert_ne!(r.target(), Some(b));
        assert_eq!(r.peel_to_commit().unwrap().id(), b);
        create(p, &b.to_string(), "plain", Some("  ")).unwrap();
        assert_eq!(repo.find_reference("refs/tags/plain").unwrap().target(), Some(b));

        assert!(create(p, &b.to_string(), "v1", None).unwrap_err().contains("already exists"));
        assert!(create(p, &b.to_string(), "bad name", None).unwrap_err().contains("not a valid"));
        assert!(create(p, &b.to_string(), "", None).is_err());
        assert!(create(p, "nope", "x", None).unwrap_err().contains("not a commit id"));

        delete(p, "v1").unwrap();
        delete(p, "release/2").unwrap();
        assert!(repo.find_reference("refs/tags/v1").is_err());
        assert!(repo.find_reference("refs/tags/release/2").is_err());
        assert!(delete(p, "v1").unwrap_err().contains("no tag"));
        let mut left: Vec<_> = list(p).unwrap().into_iter().map(|t| t.name).collect();
        left.sort();
        assert_eq!(left, ["plain"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
