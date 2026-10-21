# Commit details, diffs and file history

## Commit details
- Click a commit in the graph: the right panel switches to that commit's message, author, date,
  parents and the files it changed (added, modified, deleted, renamed with their old path).
  Merge commits are shown against their first parent.
- Click a changed file: the centre area shows the file with the commit's changes marked in
  place (added lines in green with `+`, removed lines in red with `-`, old and new line
  numbers). Three view buttons in the diff header choose how it is shown: **Full file** (the whole file, changes marked
  in place), **Changes** (only the changed hunks with three lines of context) and **Side by side** (the old file on the left, the new file on the right, line by line: a removed
  line faces the added line that replaced it, and a blank hatched area faces a line that only exists on the other side). The choice is remembered. In the side-by-side view both
  halves scroll sideways together; right-click a changed half for **Stage line** / **Discard line** (one line at a time there, no text-selection of several lines), and the
  hunk buttons and the ▲ / ▼ change navigation work as in the other views. The scrollbar marks are orange for a changed line (removed and added side by side). Binary and very large files are not previewed, and diffs are
  capped at 20,000 lines. In the full-file view, green and red marks along the scrollbar show where the added and removed lines are
  (the scrollbar itself still works as usual). The ▲ / ▼ buttons in the diff header jump to the previous / next change (hunk) and show
  which one you are at ("3 / 7"); a button is greyed out when there is nothing further in that direction. **Back to graph** returns to the commit graph exactly where you left it.
- Click a file in the staging panel (Unstaged or Staged): the centre area shows its diff the same
  way. Unstaged compares the index with the file on disk (untracked files show as all added),
  Staged compares HEAD with the index. It refreshes when you edit, stage or return to the window,
  and keeps its scroll position.
- Every run of changed lines is a **hunk** with its own heading ("Hunk 2 of 5 · +3 -1"), in all diff views. On
  the *unstaged* diff of a tracked file each heading has **Stage hunk** (puts just that hunk into the staging
  area) and **Discard hunk** (removes it from the file after a confirmation; **Undo** brings it back); on the
  *staged* diff each heading has **Unstage hunk** (takes just that hunk back out of the staging area, the
  file on disk is untouched; a newly added file is removed from the index, a staged deletion comes back). Line endings,
  including CRLF files and a missing final newline, are preserved, and a hunk whose file changed since the diff
  was shown is refused instead of applied to the wrong lines. New (untracked) files and binary files have no
  hunk buttons: stage the whole file from the list.
- **Right-click an added or removed line** in the *unstaged* diff of a tracked file for **Stage line** (puts just that one line into the staging area; the rest of its
  hunk stays unstaged, and **Undo** takes it back) and **Discard line** (throws away just that line after a confirmation: an added line is removed from the file, a
  removed line is put back where it was, with the file's own line ending; **Undo** brings it back). Context lines, the staged diff and new files have no line menu.
  **Several lines at once:** select them with the mouse (drag over the code, as when copying text) and right-click inside the selection: the menu then says **Stage 4 lines** /
  **Discard 4 lines** (the number of added and removed lines in the selection; context lines and hunk headings are not counted, and the selection may span several hunks). It is one
  step to **Undo**, and discarding asks once for the whole selection. Right-click outside the selection to act on a single line. The selection works within the part of the
  file that is currently drawn (the visible rows plus a margin): select in steps for a very long stretch.
- While you have uncommitted changes, a notice at the top of the panel says how many files
  changed in the working directory, with a **View changes** button that closes the commit view
  and returns to the staging panel (your draft commit message is kept). The × does the same.

## File history
- Right-click a changed file in the staging panel (staged or unstaged) and choose **File history**. The centre area then lists every commit that
  changed that file, newest first (up to 500), with a badge for what the commit did to it (new, modified, renamed, deleted), the short id, message,
  author and date. The history follows the file across renames. Click a commit to see exactly what it changed in the file, in the usual diff view
  (**Back to file history** or Esc returns to the list at the same scroll position; Esc in the list returns to the graph). A file that is new and
  not committed yet has no history, so the item is greyed out.
- The same **File history** item is in the right-click menu of a file in a commit's file list (right panel). The history then starts at exactly that commit and
  leaves out every later change to the file (the title says "File history up to <id>"); the file is followed back across renames from the name it had in that commit.
  Not offered for stash files, and not while the interactive rebase screen is open.

Back to the [feature index](../../FEATURES.md).
