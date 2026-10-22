# Creating commits

## Creating commits
- **Changes panel** with Unstaged and Staged file lists and status badges
  (A added, M modified, D deleted, T type change, ! conflicted).
- A **trash** button beside **Stage all** in the Unstaged list discards every unstaged change at once (modified files go back to the staged version, deleted files
  come back, untracked files are deleted), after a confirmation that counts the files. Files with merge conflicts are left alone. **Undo** brings it all back until the app is closed.
- Stage or unstage individual files, or all at once. Deleted files and repos without
  any commits yet are supported.
- Commit message box. Commit with the button or `Ctrl`/`Cmd` + `Enter`.
- **Amend previous commit** switch above the message box: pre-fills the last commit's
  message and replaces that commit instead of creating a new one. Staged changes are folded
  in, and a message-only amend works with nothing staged. The author is kept and the
  committer becomes you, like `git commit --amend`. If the commit is already pushed, a
  warning says amending it needs a force push.
- Commit is refused when there is no message, nothing is staged, conflicts are
  unresolved, or `user.name` / `user.email` aren't configured. Errors are shown in the panel.
- Finishing an in-progress merge records the merge parents and clears the merge state.
- The graph, branch label and file lists reload automatically when the repo changes, even
  from outside the app (terminal, editor, other tools): the `.git` folder is watched for
  HEAD, index and ref changes. File lists also refresh when
  the window regains focus, so edits made in your editor show up.

## Right-click on a changed file
- In the **Unstaged** list: *Stage*, *Discard changes…* (or *Delete file…* for an untracked file, *Restore file…* for one you
  deleted; asks first and cannot be undone; a staged version of the file is kept) and *Stash*.
- In the **Staged** list: *Unstage* and *Stash*.
- *Stash* moves just that file (its staged and unstaged edits, or the untracked file itself) into a new stash named
  `WIP on <branch>: ...`; every other change stays in the working directory. Discard is not offered for conflicted files.

Back to the [feature index](../../FEATURES.md).
