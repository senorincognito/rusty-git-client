# Undo and Redo: design notes

Contributor notes moved out of CLAUDE.md: how it is built and why. What the user sees is described in `docs/features/`. The undo journal and how to make a new command undoable.

- **Undo / Redo** (`undo/mod.rs`, `UndoButtons` in the title bar + Ctrl/Cmd+Z): every command that changes the repository is wrapped in `undo::recorded(&path, label, Kind, || ...)`
  (the closure starts the real work *after* the "before" snapshot; the entry is pushed only if the op succeeded and the state differs). A `Snapshot` = HEAD (branch name or
  detached id), all local branch tips, the stash list (id + message), and for some kinds the index and the whole working directory as **trees** (built in memory with
  `update_all` + `add_all`, so untracked files are included, ignored ones not; objects stay in the odb). `Kind` decides what is captured and restored: `Keep` (refs only: commit,
  amend, soft reset, branch create/rename/delete, stash drop; undoing a commit leaves the content staged because the index is left alone), `Switch` (refs + safe checkout of the old
  tree, never overwriting newer edits: merge, ff, pull, rebase family, rename, drop, checkout), `Index` (+ index tree: stage/unstage, hunks, mixed reset), `Full` (+ index and workdir trees,
  restored by `restore_workdir` which rewrites changed files and deletes added ones: discard, stash create/pop/apply, hard reset). Undo/Redo first compare the *current* snapshot with the
  entry's expected one; any difference (the terminal did something) = nothing happens, the journal for that repo is cleared. Stashes are restored by dropping them all and
  `git stash store -m <message> <id>` in order (the stash commits outlive a drop). The journal is in memory per repo (`JOURNALS`, keyed by the git dir), max 50 steps; a new action clears
  Redo. Not recorded: fetch, push, force push, remote edits (they can't be taken back locally). To make a new command undoable wrap it and choose the `Kind`; add a label that reads like
  "Undo: <label>". Untested in the running UI; the journal logic itself is unit-tested with real repositories.
