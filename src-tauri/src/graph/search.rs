use git2::Repository;
use serde::Serialize;

use super::{cache, inputs, take_order, Cached, Layout};

/// More hits than anyone steps through; the rest is reported as cut off.
const MAX_HITS: usize = 1000;
/// Shorter hex strings are common words ("add", "bad") and would match a random share of all ids.
const MIN_SHA_PREFIX: usize = 4;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: String,
    /// Position among the graph's commit rows (the uncommitted-changes row not counted), so the UI knows how far to load.
    pub index: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    /// More commits matched than `hits` holds.
    pub truncated: bool,
}

/// Commits shown in the graph whose message contains `query` (ignoring case), or whose id starts with it. In graph
/// order, so stepping through the hits walks down the graph.
fn search(path: &str, query: &str) -> Result<SearchResult, String> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(SearchResult { hits: Vec::new(), truncated: false });
    }
    let sha = needle.len() >= MIN_SHA_PREFIX && needle.bytes().all(|b| b.is_ascii_hexdigit());

    let mut repo = Repository::discover(path).map_err(|e| e.message().to_string())?;
    let inputs = inputs(&mut repo)?;
    let (order, saved) = take_order(&repo, &inputs)?;

    let mut hits = Vec::new();
    let mut truncated = false;
    let mut failed = None;
    for (index, &oid) in order.iter().enumerate() {
        let id = oid.to_string();
        let matched = (sha && id.starts_with(&needle))
            || match repo.find_commit(oid) {
                Ok(commit) => String::from_utf8_lossy(commit.message_bytes()).to_lowercase().contains(&needle),
                Err(e) => {
                    failed = Some(e.message().to_string());
                    break;
                }
            };
        if matched {
            if hits.len() == MAX_HITS {
                truncated = true;
                break;
            }
            hits.push(SearchHit { id, index });
        }
    }

    // Hand the order back so the graph's next page doesn't walk the history again. A fresh order gets an empty
    // layout: it is only resumed by a page starting at its `pos`, which is 0, and a first page always starts over.
    let layout = saved.unwrap_or_else(|| Layout::new(inputs.head));
    cache().insert(repo.path().to_path_buf(), Cached { key: inputs.key, order, layout });
    match failed {
        Some(e) => Err(e),
        None => Ok(SearchResult { hits, truncated }),
    }
}

#[tauri::command]
pub async fn search_commits(path: String, query: String) -> Result<SearchResult, String> {
    tauri::async_runtime::spawn_blocking(move || search(&path, &query)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Oid, Signature};

    fn commit(repo: &Repository, msg: &str, parents: &[Oid], at: i64) -> Oid {
        let sig = Signature::new("t", "t@example.com", &git2::Time::new(at, 0)).unwrap();
        let tree = repo.find_tree(repo.treebuilder(None).unwrap().write().unwrap()).unwrap();
        let ps: Vec<_> = parents.iter().map(|o| repo.find_commit(*o).unwrap()).collect();
        let refs: Vec<_> = ps.iter().collect();
        repo.commit(Some("refs/heads/main"), &sig, &sig, msg, &tree, &refs).unwrap()
    }

    #[test]
    fn finds_messages_and_id_prefixes_in_graph_order() {
        let dir = std::env::temp_dir().join(format!("gc-search-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();
        let a = commit(&repo, "Add login form", &[], 1_700_000_000);
        let b = commit(&repo, "Fix typo\n\nThe LOGIN button said logn.", &[a], 1_700_000_001);
        let c = commit(&repo, "Unrelated", &[b], 1_700_000_002);
        repo.set_head("refs/heads/main").unwrap();
        let p = dir.to_str().unwrap();
        let ids = |r: SearchResult| r.hits.iter().map(|h| (h.id.clone(), h.index)).collect::<Vec<_>>();

        // Case-insensitive, the body counts too, newest first with the graph's row positions.
        assert_eq!(ids(search(p, "  login ").unwrap()), [(b.to_string(), 1), (a.to_string(), 2)]);
        // An id prefix (any case) finds the commit.
        let prefix = c.to_string()[..7].to_uppercase();
        assert_eq!(ids(search(p, &prefix).unwrap()), [(c.to_string(), 0)]);
        // Nothing to look for, nothing found.
        assert!(search(p, "   ").unwrap().hits.is_empty());
        assert!(search(p, "no such text").unwrap().hits.is_empty());

        // The order handed back to the cache still serves the graph.
        let g = super::super::build_graph(p, 0, 10).unwrap();
        assert_eq!(g.rows.len(), 3);
        assert_eq!(g.rows[0].id, c.to_string());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
