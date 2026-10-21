# Commit graph

## Commit graph
- All local branches, remote branches and tags, plus a detached HEAD, newest first.
  Stash and notes refs are left out.
- Coloured lane layout computed in Rust: branches, merges and joins are drawn as curved
  lines per row.
- Ref chips on each commit (current branch highlighted; branches, remotes and tags
  styled separately).
- The commit you are on (HEAD) stands out: a ring around its node, an accent bar and a faint tint on its row, and a bold message. The chip of the checked-out
  branch is filled with a halo. Commits that are not part of the checked-out branch's history (other branches' own commits) are shown faded, so the
  current branch's history is easy to follow.
- Columns for message, author, date and short hash.
- Virtualised rendering, with history loaded in pages of 1000 as you scroll.
- While there are uncommitted changes, a dashed "N file changes in working directory" row sits on top
  of the graph, joined to the commit you are on. Clicking it shows those changes (the staging panel) in
  the right panel; it is highlighted whenever no commit is selected. The row updates when you come back
  to the window after editing files elsewhere.
- Click a row to select it.

## Commit graph context menu
- A commit that is the tip of a branch, local or remote, has **Check out `<branch>`** at the top of its right-click menu (a **Check out** group listing the
  branches when several end at the same commit). **Double-clicking** the commit does the same: it switches to the local branch, or, when only remote branches
  end there, to the remote branch (a local branch that tracks it is created, like checking it out from the sidebar). With several remote branches and no local one,
  double-click does nothing and the menu lets you choose. The checked-out branch is not offered. A checkout never overwrites uncommitted changes; if they are in the way
  you get an error and nothing changes. **Undo** switches back. While a branch switch runs (from the graph or the sidebar) the whole left panel is locked and shows a loading state, and a failure is shown in a dialog.
- Right-click a commit: **Checkout this commit** (detaches HEAD there; safe, uncommitted changes in the way stop it), **Cherry pick this commit** and
  **Revert commit** (each asks first, then adds a new commit on the checked-out branch; merge commits use their first parent; a conflict cancels it and
  names the files; both need a checked-out branch; Undo takes them back) and **Copy commit SHA** (full id to the clipboard).
- Right-click a commit that is the tip of another branch (local or remote): **Merge <branch> into the current branch** and **Rebase the current
  branch onto <branch>** (same confirmations and conflict handling as in the sidebar; a submenu when several branches sit on the commit).
- Right-click a commit that carries a branch label: **Delete branch `<name>`** (local) or **Delete remote branch `<name>`** (removed on the server) is the very first item
  (a **Delete branch** submenu when several branches sit on the commit). It asks first, with the same warnings as in the left panel, and is greyed out for the
  checked-out branch; a remote branch that is the checked-out branch's upstream is refused. A local delete can be undone, a remote one cannot.
- Right-click a commit that carries a branch label (local or remote, the checked-out branch too): **Copy branch name** comes next (a submenu when several
  branches sit on the commit).
- Right-click a branch in the sidebar (local or remote): **Copy branch name** puts the name (`feature/x`, `origin/feature/x`) on the clipboard.
- Right-click a commit: **Rename commit** opens an editor in the right panel with the full
  message and **Update** / **Cancel** buttons (Esc cancels, Ctrl+Enter updates). Only
  commits on the current branch can be renamed.
- Update rewrites the commit's message and rebuilds every later commit on the branch on top
  of it (same files, authors and dates, new ids), then moves the branch. Files and the index
  are untouched, so there can be no conflicts. Other branches keep their old history. The
  panel warns when later commits are rewritten and when the commit is already pushed (a
  force push is then needed).

Back to the [feature index](../../FEATURES.md).
