# Undo and Redo

- The **↶ Undo** and **↷ Redo** buttons in the title bar (after the Branch button) take back and repeat what you did in the app, one step at a time (the last
  50). Hover a button to see what it would take back, e.g. "Undo: Merge feature". Also `Ctrl`/`Cmd` + `Z` and `Ctrl`/`Cmd` + `Shift` + `Z` (or `Ctrl` + `Y`).
- What can be undone: **commit** and **amend** (the changes are staged again), **merge**, **fast-forward**, **pull**, **reset** (a hard reset also brings back the
  uncommitted work it threw away), **interactive rebase**, **rename** and **drop commit**, **check out**, **create / rename / delete branch**, every **stash** action
  (create, apply, pop, delete: the files and the stash list go back), **stage / unstage** (files and hunks) and **discard** (files and hunks, including deleted
  untracked files).
- Undo only acts while the repository is exactly as that step left it. If something else changed it in the meantime (the terminal, another program), nothing is
  touched, you are told why, and the undo history is cleared. Steps that move files (merge, fast-forward, checkout...) never overwrite edits you made since: Undo stops
  and says so; commit, stash or discard them and try again. Doing something new ends the Redo history.
- Not undoable: fetch, push and force push, and anything that edits remotes (add, remove, change URL, rename or delete remote branches). The history lives only
  while the app is open and is kept per repository. Undo is off while the interactive rebase screen is open.

Back to the [feature index](../../FEATURES.md).
