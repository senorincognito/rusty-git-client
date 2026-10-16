pub mod fast_forward;
pub mod merge;
pub mod rebase_onto;
pub mod reset;

use std::collections::{HashMap, HashSet};

use git2::{Oid, Repository, Sort};
use serde::Serialize;

const COLOR_COUNT: usize = 8;
/// Id of the pseudo commit that stands for the uncommitted changes.
pub const WIP_ID: &str = "WIP";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefLabel {
    pub name: String,
    /// "branch" | "remote" | "tag"
    pub kind: &'static str,
    pub is_head: bool,
}

/// A line entering the node from the top edge of its row.
#[derive(Serialize)]
pub struct Edge {
    pub col: usize,
    pub color: usize,
    /// Drawn dashed: the lane that leads from the uncommitted-changes row to the commit it sits on.
    pub dashed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRow {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author: String,
    pub email: String,
    /// Unix seconds.
    pub time: i64,
    pub parents: Vec<String>,
    /// HEAD or an ancestor of it: the commits that can be rewritten on the current branch.
    pub on_head: bool,
    /// A stash: drawn as a hollow node hanging off the commit it was made on.
    pub is_stash: bool,
    /// The pseudo commit for uncommitted changes, shown on top when there are any.
    pub is_wip: bool,
    pub refs: Vec<RefLabel>,
    pub col: usize,
    pub color: usize,
    /// Lanes ending at this node, drawn from the top edge of the row.
    pub top: Vec<Edge>,
    /// Lanes passing straight through the row.
    pub through: Vec<Edge>,
    /// Lines from this node down to the bottom edge of the row.
    pub bottom: Vec<Edge>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    pub rows: Vec<GraphRow>,
    pub max_lanes: usize,
    /// True when history continues beyond the returned rows.
    pub has_more: bool,
}

#[derive(Clone, Copy)]
struct Lane {
    oid: Oid,
    color: usize,
    dashed: bool,
}

/// Ref labels per commit, and the commits that start the walk.
type RefsAndTips = (HashMap<Oid, Vec<RefLabel>>, Vec<Oid>);

fn collect_refs(repo: &Repository) -> Result<RefsAndTips, git2::Error> {
    let mut labels: HashMap<Oid, Vec<RefLabel>> = HashMap::new();
    let mut tips = Vec::new();
    let head_name = repo.head().ok().and_then(|h| h.name().ok().map(str::to_string));

    for r in repo.references()?.flatten() {
        let Ok(full) = r.name() else { continue };
        let (kind, short) = if let Some(s) = full.strip_prefix("refs/heads/") {
            ("branch", s)
        } else if let Some(s) = full.strip_prefix("refs/remotes/") {
            if s.ends_with("/HEAD") {
                continue;
            }
            ("remote", s)
        } else if let Some(s) = full.strip_prefix("refs/tags/") {
            ("tag", s)
        } else {
            continue; // stash, notes, etc.
        };
        let Ok(commit) = r.peel_to_commit() else { continue };
        let oid = commit.id();
        tips.push(oid);
        labels.entry(oid).or_default().push(RefLabel {
            name: short.to_string(),
            kind,
            is_head: head_name.as_deref() == Some(full),
        });
    }

    // Detached HEAD isn't covered by a branch ref.
    if let Ok(h) = repo.head() {
        if !h.is_branch() {
            if let Some(oid) = h.target() {
                tips.push(oid);
                labels.entry(oid).or_default().push(RefLabel { name: "HEAD".into(), kind: "branch", is_head: true });
            }
        }
    }

    // Stable order inside a commit: current branch first, then branches, remotes, tags.
    for list in labels.values_mut() {
        list.sort_by_key(|l| {
            (
                !l.is_head,
                match l.kind {
                    "branch" => 0,
                    "remote" => 1,
                    _ => 2,
                },
            )
        });
    }
    Ok((labels, tips))
}

fn first_free(lanes: &mut Vec<Option<Lane>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(i) => i,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

fn build_graph(path: &str, limit: usize) -> Result<Graph, String> {
    let mut repo = Repository::discover(path).map_err(|e| e.message().to_string())?;
    let (mut labels, mut tips) = collect_refs(&repo).map_err(|e| e.message().to_string())?;

    // Stashes show up as nodes hanging off the commit they were made on. Their second and third
    // parents (the saved index and the untracked files) are implementation details: not drawn.
    let mut stash_ids: HashSet<Oid> = HashSet::new();
    let mut stash_internal: HashSet<Oid> = HashSet::new();
    for stash in crate::sidebar::stash::list_stashes(&mut repo)? {
        let Ok(oid) = Oid::from_str(&stash.id) else { continue };
        if let Ok(commit) = repo.find_commit(oid) {
            stash_internal.extend(commit.parent_ids().skip(1));
        }
        stash_ids.insert(oid);
        tips.push(oid);
        labels.entry(oid).or_default().push(RefLabel {
            name: format!("stash@{{{}}}", stash.index),
            kind: "stash",
            is_head: false,
        });
    }

    let mut walk = repo.revwalk().map_err(|e| e.message().to_string())?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME).map_err(|e| e.message().to_string())?;
    for tip in tips {
        walk.push(tip).map_err(|e| e.message().to_string())?;
    }

    let mut lanes: Vec<Option<Lane>> = Vec::new();
    let mut next_color = 0usize;
    let mut new_color = || {
        let c = next_color % COLOR_COUNT;
        next_color += 1;
        c
    };

    // Rows come children-first, so one pass can tell which commits are ancestors of HEAD.
    let mut head_ancestry: HashSet<Oid> = HashSet::new();
    if let Some(head) = repo.head().ok().and_then(|h| h.peel_to_commit().ok()) {
        head_ancestry.insert(head.id());
    }

    let mut rows: Vec<GraphRow> = Vec::new();
    let mut max_lanes = 0usize;
    let mut has_more = false;

    // Uncommitted changes appear as a pseudo commit on top, joined by a dashed lane to the commit
    // HEAD points at (so it lands in that commit's lane). Not on a branch without commits.
    if let Some(head) = repo.head().ok().and_then(|h| h.peel_to_commit().ok()) {
        let changed = crate::changes::status_of(&repo)?.len();
        if changed > 0 {
            let color = new_color();
            lanes.push(Some(Lane { oid: head.id(), color, dashed: true }));
            max_lanes = 1;
            rows.push(GraphRow {
                id: WIP_ID.to_string(),
                short_id: String::new(),
                summary: format!("{changed} file change{} in working directory", if changed == 1 { "" } else { "s" }),
                author: String::new(),
                email: String::new(),
                time: 0,
                parents: vec![head.id().to_string()],
                on_head: false,
                is_stash: false,
                is_wip: true,
                refs: Vec::new(),
                col: 0,
                color,
                top: Vec::new(),
                through: Vec::new(),
                bottom: vec![Edge { col: 0, color, dashed: true }],
            });
        }
    }

    for oid in walk {
        let oid = oid.map_err(|e| e.message().to_string())?;
        if stash_internal.contains(&oid) {
            continue;
        }
        if rows.len() >= limit {
            has_more = true;
            break;
        }
        let commit = repo.find_commit(oid).map_err(|e| e.message().to_string())?;
        // A stash only continues along the commit it was made on.
        let parent_ids: Vec<Oid> = if stash_ids.contains(&oid) {
            commit.parent_ids().take(1).collect()
        } else {
            commit.parent_ids().collect()
        };

        let matches: Vec<usize> =
            lanes.iter().enumerate().filter(|(_, l)| l.is_some_and(|l| l.oid == oid)).map(|(i, _)| i).collect();

        let (col, color) = match matches.first() {
            Some(&i) => (i, lanes[i].unwrap().color),
            None => {
                let i = first_free(&mut lanes);
                (i, new_color())
            }
        };

        let top: Vec<Edge> = matches
            .iter()
            .map(|&i| Edge { col: i, color: lanes[i].unwrap().color, dashed: lanes[i].unwrap().dashed })
            .collect();
        let through: Vec<Edge> = lanes
            .iter()
            .enumerate()
            .filter(|(i, l)| l.is_some() && !matches.contains(i))
            .map(|(i, l)| Edge { col: i, color: l.unwrap().color, dashed: l.unwrap().dashed })
            .collect();
        for &i in &matches {
            lanes[i] = None;
        }

        let mut bottom = Vec::new();
        for (n, pid) in parent_ids.iter().copied().enumerate() {
            let existing = lanes.iter().position(|l| l.is_some_and(|l| l.oid == pid));
            match (existing, n) {
                // Parent already has a lane (another branch reaches it): join that lane.
                (Some(j), _) => {
                    bottom.push(Edge { col: j, color: lanes[j].unwrap().color, dashed: lanes[j].unwrap().dashed })
                }
                // First parent continues in this node's lane.
                (None, 0) => {
                    lanes[col] = Some(Lane { oid: pid, color, dashed: false });
                    bottom.push(Edge { col, color, dashed: false });
                }
                // Additional parents (merge) open a new lane.
                (None, _) => {
                    let slot = first_free(&mut lanes);
                    let c = new_color();
                    lanes[slot] = Some(Lane { oid: pid, color: c, dashed: false });
                    bottom.push(Edge { col: slot, color: c, dashed: false });
                }
            }
        }

        max_lanes = max_lanes.max(lanes.len()).max(col + 1);
        while matches!(lanes.last(), Some(None)) {
            lanes.pop();
        }

        let on_head = head_ancestry.contains(&oid);
        if on_head {
            head_ancestry.extend(parent_ids.iter().copied());
        }

        let author = commit.author();
        rows.push(GraphRow {
            id: oid.to_string(),
            short_id: oid.to_string()[..7].to_string(),
            summary: commit.summary().ok().flatten().unwrap_or("").to_string(),
            author: author.name().unwrap_or("").to_string(),
            email: author.email().unwrap_or("").to_string(),
            time: commit.time().seconds(),
            parents: parent_ids.iter().map(|p| p.to_string()).collect(),
            on_head,
            is_stash: stash_ids.contains(&oid),
            is_wip: false,
            refs: labels.remove(&oid).unwrap_or_default(),
            col,
            color,
            top,
            through,
            bottom,
        });
    }

    Ok(Graph { rows, max_lanes, has_more })
}

/// Commit graph over all branches, remotes and tags, newest first, with lane layout.
/// Async so the (blocking) walk runs off the main thread.
#[tauri::command]
pub async fn get_graph(path: String, limit: usize) -> Result<Graph, String> {
    tauri::async_runtime::spawn_blocking(move || build_graph(&path, limit)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Signature;

    // Every commit gets a later timestamp than the one before: with equal times (several commits in one second)
    // the order of sibling commits in the graph is undefined and the layout assertions flap.
    fn commit(repo: &Repository, msg: &str, parents: &[Oid], update: Option<&str>) -> Oid {
        static CLOCK: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1_700_000_000);
        let at = CLOCK.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let sig = Signature::new("t", "t@example.com", &git2::Time::new(at, 0)).unwrap();
        let tree = repo.find_tree(repo.treebuilder(None).unwrap().write().unwrap()).unwrap();
        let ps: Vec<_> = parents.iter().map(|o| repo.find_commit(*o).unwrap()).collect();
        let refs: Vec<_> = ps.iter().collect();
        repo.commit(update, &sig, &sig, msg, &tree, &refs).unwrap()
    }

    #[test]
    fn branch_and_merge_layout() {
        let dir = std::env::temp_dir().join(format!("gc-graph-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();

        //  M  (merge of A2 and B1)
        //  |\
        //  A2 B1
        //  |/
        //  A1
        let a1 = commit(&repo, "a1", &[], Some("refs/heads/main"));
        // b1 is older than a2, so a2 is listed first and the topic branch gets the second lane.
        let b1 = commit(&repo, "b1", &[a1], Some("refs/heads/topic"));
        let a2 = commit(&repo, "a2", &[a1], Some("refs/heads/main"));
        let m = commit(&repo, "m", &[a2, b1], Some("refs/heads/main"));
        repo.set_head("refs/heads/main").unwrap();

        let g = build_graph(dir.to_str().unwrap(), 100).unwrap();
        let ids: Vec<_> = g.rows.iter().map(|r| r.id.clone()).collect();
        assert_eq!(g.rows.len(), 4);
        assert_eq!(ids[0], m.to_string());
        assert_eq!(ids[3], a1.to_string());
        assert!(!g.has_more);

        // Merge node opens a second lane for its second parent.
        assert_eq!(g.rows[0].bottom.len(), 2);
        assert_eq!(g.max_lanes, 2);
        // The branch commit joins the existing lane of its parent on its own row.
        let b1_row = g.rows.iter().find(|r| r.id == b1.to_string()).unwrap();
        assert_eq!(b1_row.col, 1);
        assert_eq!(b1_row.bottom.len(), 1);
        assert_eq!(b1_row.bottom[0].col, 0);
        // Root commit receives the single remaining lane.
        assert_eq!(g.rows[3].top.len(), 1);
        assert!(g.rows[3].bottom.is_empty());
        // Branch labels land on tips, HEAD flagged.
        assert!(g.rows[0].refs.iter().any(|r| r.name == "main" && r.is_head));
        assert!(g.rows.iter().any(|r| r.refs.iter().any(|l| l.name == "topic")));

        // Limit truncates and reports more history.
        let g = build_graph(dir.to_str().unwrap(), 2).unwrap();
        assert_eq!(g.rows.len(), 2);
        assert!(g.has_more);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn marks_commits_reachable_from_head() {
        let dir = std::env::temp_dir().join(format!("gc-onhead-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();

        // main: a - b      side: a - s  (side is not an ancestor of HEAD)
        let a = commit(&repo, "a", &[], Some("refs/heads/main"));
        let b = commit(&repo, "b", &[a], Some("refs/heads/main"));
        let s = commit(&repo, "s", &[a], Some("refs/heads/side"));
        repo.set_head("refs/heads/main").unwrap();

        let g = build_graph(dir.to_str().unwrap(), 100).unwrap();
        let on_head = |id: Oid| g.rows.iter().find(|r| r.id == id.to_string()).unwrap().on_head;
        assert!(on_head(b) && on_head(a));
        assert!(!on_head(s));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stashes_are_nodes_off_their_base_commit() {
        let dir = std::env::temp_dir().join(format!("gc-stashgraph-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut repo = Repository::init(&dir).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "S").unwrap();
        cfg.set_str("user.email", "s@example.com").unwrap();
        cfg.set_str("core.autocrlf", "false").unwrap(); // the result must not depend on the machine's git config

        let a = commit(&repo, "a", &[], Some("refs/heads/main"));
        let b = commit(&repo, "b", &[a], Some("refs/heads/main"));
        repo.set_head("refs/heads/main").unwrap();
        std::fs::write(dir.join("u.txt"), "untracked").unwrap();
        let stash = crate::sidebar::stash::save_stash(&mut repo, Some("wip")).unwrap();

        let g = build_graph(dir.to_str().unwrap(), 100).unwrap();
        // Two commits plus one stash: the saved-index and untracked-files commits stay hidden.
        assert_eq!(g.rows.len(), 3, "{:?}", g.rows.iter().map(|r| &r.summary).collect::<Vec<_>>());
        let row = g.rows.iter().find(|r| r.id == stash.to_string()).unwrap();
        assert!(row.is_stash && !row.on_head);
        assert_eq!(row.parents, [b.to_string()]);
        assert_eq!((row.refs.len(), row.refs[0].name.as_str(), row.refs[0].kind), (1, "stash@{0}", "stash"));
        assert!(g.rows.iter().filter(|r| r.is_stash).count() == 1);
        // The stash sits above the commit it hangs off.
        let pos = |id: Oid| g.rows.iter().position(|r| r.id == id.to_string()).unwrap();
        assert!(pos(stash) < pos(b) && pos(b) < pos(a));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn uncommitted_changes_are_a_pseudo_commit_on_top() {
        let dir = std::env::temp_dir().join(format!("gc-wip-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();

        // No commits yet: nothing to attach the pseudo commit to.
        std::fs::write(dir.join("early.txt"), "x").unwrap();
        assert!(build_graph(dir.to_str().unwrap(), 100).unwrap().rows.is_empty());
        std::fs::remove_file(dir.join("early.txt")).unwrap();

        let a = commit(&repo, "a", &[], Some("refs/heads/main"));
        let b = commit(&repo, "b", &[a], Some("refs/heads/main"));
        repo.set_head("refs/heads/main").unwrap();

        // A clean working directory has no pseudo commit.
        let g = build_graph(dir.to_str().unwrap(), 100).unwrap();
        assert_eq!(g.rows.len(), 2);
        assert!(g.rows.iter().all(|r| !r.is_wip));

        // One changed file: a row on top, joined by a dashed lane to HEAD's commit.
        std::fs::write(dir.join("one.txt"), "1").unwrap();
        let g = build_graph(dir.to_str().unwrap(), 100).unwrap();
        assert_eq!(g.rows.len(), 3);
        let wip = &g.rows[0];
        assert!(wip.is_wip && !wip.is_stash && !wip.on_head);
        assert_eq!((wip.id.as_str(), wip.summary.as_str()), ("WIP", "1 file change in working directory"));
        assert_eq!(wip.parents, [b.to_string()]);
        assert!(wip.bottom.len() == 1 && wip.bottom[0].dashed && wip.top.is_empty());
        let head_row = &g.rows[1];
        assert_eq!(head_row.id, b.to_string());
        assert_eq!(head_row.col, wip.col, "the pseudo commit sits in HEAD's lane");
        assert!(head_row.top.len() == 1 && head_row.top[0].dashed);
        assert!(head_row.bottom.iter().all(|e| !e.dashed), "the real history below is solid");
        assert!(g.rows[2].top.iter().all(|e| !e.dashed));

        // More files: the count follows, and the real rows are unaffected.
        std::fs::write(dir.join("two.txt"), "2").unwrap();
        let g = build_graph(dir.to_str().unwrap(), 100).unwrap();
        assert_eq!(g.rows[0].summary, "2 file changes in working directory");
        assert_eq!(g.rows.iter().filter(|r| r.is_wip).count(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
