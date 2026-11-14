# Merge, rebase and history rewrites: design notes

Contributor notes moved out of CLAUDE.md: how it is built and why. What the user sees is described in `docs/features/`. Covers interactive rebase, rename, drop, merge (and its conflict view), rebase onto, fast-forward and reset.

- **Interactive rebase** (graph context menu, `features/rebase/InteractiveRebase`): a full screen that covers the sidebar
  and the centre (`RepoView` wraps both in `.workarea`; they stay mounted, `visibility: hidden` + `inert`, so the graph
  keeps its scroll position); the right panel stays: `Changes`, or `CommitDetail` of the commit clicked in the list (`selectedId`/`onSelectCommit`, the shared
  `selectedCommit` state). A file clicked there opens `FileDiff` in `.rebase-diff` over the rebase screen (the normal centre
  `FileDiff` is suppressed while rebasing); `diffOpen` stops the rebase screen's own Escape. Working diffs are ignored meanwhile.
  The clicked commit is the base and is not edited; `rebase_plan` lists the commits after it on HEAD's first-parent line
  (max 500, newest first) plus `headId`. Each row has an action `<select>`: **pick**, **reword** (a `Modal` popup;
  a commit only becomes "reword" when the popup is confirmed), **squash** and **drop**. `apply_rebase` (command `apply_rebase_cmd`,
  steps `{id, action, message?}`, unlisted commits are picks) groups each squashed commit with the next older non-squashed
  commit (a chain of squashes all go into the one that starts it), then rebuilds from the oldest changed group: parents of
  the group's oldest commit, tree of its newest commit, author of the oldest, you as committer, message = the older
  message + blank line + each squashed message. No cherry-picks (each commit already contains its predecessors), so no
  conflicts and no clean working directory are needed, and the tip's tree never changes. The oldest commit and merge
  commits can't be squashed (`RebaseGroup`), nor squashed into a dropped commit; it refuses when HEAD moved since the plan.
  **Drop** changes content, so from the first dropped group on `replay_group` re-creates each kept commit with
  `cherrypick_commit` onto the rebuilt parent (a dropped commit maps to its rebuilt parent in `rebuilt`); a conflict
  abandons everything with the commit and files named, a merge commit after the first drop is refused, the working
  directory must be clean and a hard reset (not `move_head_to`) finishes. Groups before the first drop still reuse trees.
  **Reorder** (context menu *Move commit up/down*, UI state `order`) is sent as `order` (ids, newest first) to
  `apply_rebase_ordered`; groups and squashes are built on the new order, `first_moved` is the first oldest-first position
  that differs from the plan, and from there `replay_group` re-creates the commits one by one on `tip` (the previous
  rebuilt commit; parents are `[tip]` in replay mode, mapped original parents otherwise). Same clean-working-directory,
  merge and conflict rules as Drop. The UI computes `problem` (an action that no longer fits after a move) to block Start.
- **Rename commit** (`history/rename.rs`; behaviour: `docs/features/commit-graph.md`) only for commits on the current branch. Rebuilds the commit and every later commit with
  identical trees/authors/dates, then moves the branch; other branches keep the old history. Warns about rewritten descendants and pushed commits.
- **Drop commit** (`history/drop.rs` `drop_plan`/`drop_commit`; behaviour: `docs/features/history-operations.md`) works for any commit on the checked-out branch's own first-parent
  line when no merge commit lies between it and the tip, and the commit has a parent. The commits after it are re-created in memory with `cherrypick_commit` (3-way
  merge, original author, committer and message; empty results are kept, not silently dropped); a conflict aborts with the commit and files named and nothing changed.
  Only then does a hard reset move the branch, index and working directory, so the working directory must be clean. The confirmation (`RepoView.dropCommit`, data from
  `get_drop_info`) warns about rewritten ids, pushed history, merge commits, other refs keeping the old history and lost signatures. Errors use `showError`.
- **Merge** (`graph/merge.rs`, via `RepoView.mergeInto`; behaviour: `docs/features/history-operations.md`): merges into the checked-out branch only, after a confirmation. Runs
  `git merge --no-edit --autostash <full ref>` (system git, so the user's identity/config apply; the ref must start with `refs/`, which also keeps it from being read as an
  option). Conflicts: see *Merge conflicts* below; a dirty tree keeps the abort-and-name-the-files policy of Pull. Fast-forwardable branches fast-forward (no merge
  commit); an already contained branch is reported with an info box. Refused on a detached HEAD or while another operation is in progress.
- **Rebase onto a branch** (`graph/rebase_onto.rs`, via `RepoView.rebaseOntoBranch`; behaviour: `docs/features/history-operations.md`): system git `rebase --autostash <full ref>`
  of the checked-out branch (not the app's own interactive-rebase engine), after a confirmation built from `get_rebase_onto_info` (`graph_ahead_behind`, plus
  `history::is_pushed` and parent counts over the replayed commits; the dialog turns to the warning style when some are pushed). Same conflict policy as Pull and Merge:
  `cancel_unfinished` aborts and names the files. A plain rebase flattens merge commits (`--rebase-merges` is not used); the dialog says so. One undo step
  (`Kind::Switch`). Offered by local branches in the sidebar and, with Merge, by any branch label (local or remote) on a graph row via `Graph.branchActionItems`
  (the checked-out branch is never a target). Remote branches in the sidebar don't offer rebase.
- **Fast-forward** (`graph/fast_forward.rs`, via `RepoView.fastForwardTo`; behaviour: `docs/features/history-operations.md`): `git merge --ff-only` semantics on the checked-out
  branch only. The target must be a strict descendant of HEAD (otherwise errors: already contained, or diverged with both counts), HEAD must be a branch, the repo
  state clean. The files are checked out safely first (uncommitted changes in the way abort it, nothing moves), then the branch ref moves. No confirmation (nothing is
  lost); errors use `showError`. The UI only pre-disables the obvious cases (stash, already on the branch's line, the branch itself); the rest is the backend's message.
- **Reset** (`graph/reset.rs`; behaviour: `docs/features/history-operations.md`): plain `git2` `reset` (Soft/Mixed/Hard) behind a confirmation whose text is built by
  `features/repo/describeReset.ts` from `get_reset_info` (commits removed with the newest few listed, commits gained, pushed count, uncommitted file count, ancestor
  or jump to another line). Allowed on a detached HEAD and on any non-stash commit, refused during a merge/rebase and on an empty repo. Hard leaves untracked files
  alone; only uncommitted changes are unrecoverable (removed commits stay in the reflog).

- **Merge conflicts** (`changes/conflicts.rs`, `features/conflicts/ConflictView`, banner in `Changes`): only `merge` and `pull_with(.., "merge")` stop on conflicts, and only with a **clean working directory**: `git merge --autostash` keeps the stashed changes hidden until the merge ends (`MERGE_AUTOSTASH`), and libgit2's commit would never restore them, so a dirty tree keeps the old behaviour (autostash, abort with `cancel_unfinished`, message adds "commit or stash first"). `merge` returns `MergeOutcome::Conflicts`, the pull returns an Ok message. The state is git's own (MERGE_HEAD + index conflicts): `get_merge_state` feeds the banner and pre-fills the message, and the existing `commit_staged` already concludes a merge (extra parents, `cleanup_state`). The view parses the *working file's* markers (so editor edits count), keeps every line's own EOL (textarea values read back with LF, so a hand-edited result is converted back to the file's EOL), and chooses per conflict in the frontend; whole-file side buttons go through system git so filters apply and a deleted side deletes the file. The commit button is enabled during a merge with nothing staged, disabled while conflicts remain. Resolve/abort are not in the undo journal (snapshots can't capture a conflicted index; the merge commit itself is undoable as usual). Sides are named as in a merge: a rebase swaps ours/theirs, so reuse for rebase needs a mapping. UI untested in the running app.
