# Worktrees: design notes

What the user sees is in `docs/features/worktrees.md`.

- **Backend** (`sidebar/worktrees.rs`): system `git worktree` through `run_git` (libgit2's worktree support is thin; `run_git` also puts the calls in the command log). `get_worktrees` parses `git worktree list --porcelain` (no `-z`: that needs git 2.36; a path with a newline would break it). Paths go through `normalize` (canonicalize, strip `\?\`) so they equal `RepoInfo.path` and a worktree opens as a tab with plain `open_repo`; git prints `C:/...` on Windows. The first record is the main working directory (`isMain`); `isCurrent` compares the open tab's workdir.
- **Where they go**: `<main>.worktrees/<branch with / as ->` beside the main working directory, never inside it (would be untracked files) and not in the app data dir (the user wants to find the folder, and agents' tools should be able to reach it). Not configurable yet.
- **Branch rule**: an existing local branch is checked out as it is, otherwise `-b <branch>` from the HEAD of the worktree the command ran in. The "already checked out elsewhere" refusal is git's own message, shown as it is.
- **Remove** never touches the branch. Without `--force` git refuses a dirty worktree; the UI recognises that message (`isDirty` in `Worktrees.tsx`, so it depends on git's wording) and offers a second confirmation for `--force`. A locked worktree needs `--force --force` and stays refused, with git's message.
- **Outside the undo journal** (like tags and files): a worktree is a folder, which snapshots don't cover.
- **Watcher** (`repo/watch.rs`): a linked worktree's git dir (`.git/worktrees/<name>`) holds only its HEAD and index, so `refs/` is watched in `commondir()`.
- **Tabs**: `RepoView` gets `onOpenRepo` / `onCloseRepo` (App's `openTab` / `closeTab`). Only the active tab is mounted, so a background worktree tab has no watcher: it reloads when you switch to it.

- **Branches held elsewhere** (`branches_held_elsewhere`, `refuse_if_held`): read with git2 by opening the main repository (`commondir()`) and every linked one via `open_from_worktree`, then each HEAD; no process, because `get_graph` and `get_local_branches` ask on every load. This is a safety check, not only wording: libgit2's `set_head` happily checks out a branch another worktree holds, which would leave two HEADs on one branch. It guards `checkout_branch`, `rename_branch` (which would otherwise leave the other worktree's HEAD dangling), `delete_branch_checked`, and `delete_synced` / the clean-up list skip such branches. `BranchInfo.worktree` and `RefLabel.worktree` (branch labels only) carry the path; the graph's own checkout still offers the label and shows the refusal.
- **Change counts** (`count_worktree_changes`): `changes::status_of` per worktree on scoped threads, keyed by path; separate from `get_worktrees` so the list never waits for a big tree. Refreshed with the list and on window focus; other worktrees' indexes aren't watched, so a commit made there shows up on the next focus.

Not built yet: launching an agent in a worktree, and a merge-back flow.
