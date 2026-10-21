# Merge, rebase, fast-forward, drop and reset

## Merging a branch
- Right-click a **local branch** (*Merge <branch> into <current>*) or a **remote branch** (*Merge <remote>/<branch> into the current branch*) to merge it into
  the checked-out branch. A dialog asks first. A merge commit is created with Git's default message, or the branch simply fast-forwards when it can; uncommitted
  changes are set aside and restored. If the branch is already contained you are told so. Conflicts are not resolved in the app yet: the merge is
  cancelled, the conflicting files are named and nothing changes. Not available on a detached HEAD.

## Rebasing onto a branch
- Right-click a **local branch** and choose **Rebase `<current>` onto `<branch>`**: the checked-out branch's own commits are replayed on top of that branch (they get new ids), like
  `git rebase <branch>`. A dialog asks first and says how many commits are replayed, how many of them are already pushed (a force push is needed afterwards) and whether merge
  commits among them get flattened. A branch with no commits of its own just moves forward; one that is already based on the target reports that nothing was done. Uncommitted
  changes are set aside and restored. Conflicts are not resolved in the app yet: the rebase is cancelled, the conflicting files are named and nothing changes. **Undo** takes a rebase
  back. Not available on a detached HEAD. (Rewriting your own history by hand is in *Interactive rebase*.)

## Fast-forward
- Right-click a **local branch** and choose **Fast-forward <current> to <branch>**, or right-click a **commit** in the graph and choose
  **Fast-forward to this commit**: the checked-out branch moves forward to it, like `git merge --ff-only`, and your files are updated.
  It only works when the target is ahead of the checked-out branch. If it already contains the commit, the histories have diverged
  (the message says by how much) or uncommitted changes are in the way, an error explains why and nothing changes. No confirmation:
  nothing is lost.

## Dropping a commit
- Right-click a commit on the current branch and choose **Drop commit** (greyed out, with a tooltip, on other
  branches' commits, stashes and the first commit). After a confirmation the commit disappears from the branch
  together with its changes in the working directory. Commits after it are re-created on top of its parent
  (new ids, same changes, authors and messages), like a drop in an interactive rebase.
- It is all-or-nothing: the replay happens in memory, and if a later commit depends on the dropped one and would
  conflict, nothing is changed and the message names the commit and files. It needs a clean working directory,
  a commit on the branch's own line with no merge commit after it, and a checked-out branch (not a detached
  HEAD). The confirmation spells out how many commits are rewritten, warns about merges and already-pushed
  history (a force push is then needed) and notes that other branches and tags keep the old history. The old
  commits stay recoverable through `git reflog` for a while.

## Resetting to a commit
- Right-click any commit, point at **Reset to this commit** (a menu group: its submenu opens to the right on
  hover, click or the right arrow key) and choose **Soft**, **Mixed** or **Hard** (the commits of other lines of
  history work too; stashes don't). Each asks first, spelling out what happens:
  - **soft** moves only the branch; the staging area and your files are untouched, so the removed commits' changes
    show up as staged changes;
  - **mixed** also resets the staging area; your files are untouched, so those changes (and anything that was
    staged) become unstaged, new files untracked;
  - **hard** also resets your files: uncommitted changes are lost for good and the removed commits' changes leave
    your files (untracked files are left alone).
- The confirmation lists the commits that leave the branch (or how far it moves forward), says how many uncommitted
  file changes a hard reset would throw away, warns when removed commits are already pushed (a force push is then
  needed), and mentions the reflog. It works on a detached HEAD too.

Back to the [feature index](../../FEATURES.md).
