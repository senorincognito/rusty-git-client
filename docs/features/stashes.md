# Stashes

- **Stash** in the title bar (between the Fetch/Pull/Push buttons and Terminal; disabled when nothing has changed) opens the same dialog as
  **Stash…** below.
- **Stash…** next to the Commit button (disabled when nothing has changed) takes an optional message
  and moves every uncommitted change (staged, unstaged and untracked files; ignored files stay) into a
  new stash, leaving the working directory clean.
- Stashes show up in the commit graph as hollow nodes hanging off the commit they were made on, labelled
  `stash@{n}`, and in a **Stashes** section in the left panel (newest first). Click one in either place
  to see its message, base commit and changed files, untracked files included, and open any file's diff.
- **Pop** (the button at the bottom of the stash's detail panel, or right-click the stash in the
  Stashes list and choose *Pop stash*) applies the stash to the working directory and removes it from the
  list, putting back staged changes as staged and restoring untracked files. It needs a clean working
  directory (the button is disabled otherwise). If the stash would conflict, the pop is undone completely
  and the stash is kept.
- **Apply stash** (right-click the stash in the Stashes list or in the commit graph) does the same as Pop but keeps the stash in the list, so
  you can apply it again elsewhere. Same rules: clean working directory, conflicts are undone completely.
- **Right-click a stash in the commit graph** for *Apply stash*, *Pop stash* and *Delete stash* (the other commit actions don't apply to stashes,
  so they are not listed). Apply and Pop are disabled, with a tooltip, while there are uncommitted changes.
- **Delete all stashes (n)** in the **⋯** menu of the Stashes headline removes every stash at once without applying any, after a confirmation that lists them. It leaves your working
  directory alone, and one **Undo** brings all the stashes back (until the app is closed).
- **Delete stash** (right-click the stash in the Stashes list) removes it without applying it, after a confirmation.
  It works with uncommitted changes in the working directory; the stash's changes are lost.

Back to the [feature index](../../FEATURES.md).
