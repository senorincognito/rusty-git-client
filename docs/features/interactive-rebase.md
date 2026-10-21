# Interactive rebase

- Right-click a commit of the current branch and choose **Interactive rebase…**. A rebase screen replaces the sidebar
  and the graph (the right panel stays): it lists every commit after the one you clicked, newest first, like
  `git rebase -i <commit>`. The commit you clicked is the base and stays as it is.
- Each commit has an action:
  - **pick** keeps the commit as it is (the default);
  - **squash** melds the commit into the one before it (the next older one in the list; the row shows "into abc1234") and
    appends its message to that commit's message, separated by a blank line. Several squashes in a row all go into the
    commit that starts the chain. The result keeps that older commit's author and parents and the content of the newest
    commit of the group. The oldest commit of the list can't be squashed (nothing before it) and neither can a merge
    commit; a merge commit can be squashed *into*. Combine it with **reword** on the older commit to set the final message;
  - **drop** removes the commit and its changes from the branch (the row is struck through). The later commits are
    replayed on top of the rebuilt history, so they get new content as well as new ids, and your files are reset to the
    result: **the working directory must be clean**. If a later commit depends on a dropped one (its changes would
    conflict), the whole rebase is cancelled with nothing changed and the commit and files are named. Merge commits
    can't be replayed, so a merge commit *after* a dropped commit blocks the rebase (merges before it are fine). A drop
    can't be combined with a commit squashed into it;
  - **reword** opens a popup with the commit message and **Cancel** / **Update message** (Ctrl/Cmd+Enter). The new
    first line shows in the list, a blue margin marks the commit, and **Edit message** opens the popup again. Choosing
    *pick* again drops the new message.
- Click a commit in the list to see the files it changed in the right panel (the panel's × or Cancel rebase returns to your
  working-directory changes); click a file there to open its diff over the rebase screen, and Back or Esc to return to the
  list. The changes shown are those of the commit as it is now, whatever you reword, squash, drop or move.
- Keyboard: `↑`/`↓` select the commit above/below (with nothing selected, `↓` picks the top commit and `↑` the bottom one),
  and `Ctrl`+`↑`/`↓` (`Cmd` on macOS) move the selected commit up/down. The arrows are left alone while you type in a
  field or use the action dropdown, with a menu or popup open, or while a file diff is shown.
- Right-click a commit in the list for the same actions as shortcuts: **Reword commit**, **Squash commit** and **Drop commit**
  (unavailable ones are greyed out with the reason), plus **Move commit up** / **Move commit down**, which change the order
  of the commits in the plan (up = newer). Moved commits get a *moved* tag. Like a drop, a new order replays the commits
  from the first moved one on (new content and ids, files reset: **the working directory must be clean**), and the whole
  rebase is cancelled with nothing changed if a commit depends on one that now comes after it (for example a change to
  a file moved below the commit that adds the file). Squash and drop are decided on the new order, and an action that no
  longer fits after a move (a squash that became the oldest commit, ...) is reported and blocks **Start rebase**.
- **Start rebase** applies everything at once; **Cancel rebase** (top or bottom, or Esc) leaves without changes.
- Reworded and squashed commits get you as committer; every commit after the oldest change is rebuilt
  with the same content, author and date (new ids). Files, the staging area and other branches are not touched. The
  footer says how many extra commits are rewritten and warns when any of them is already pushed (a force push is then
  needed).
- Only the branch's own line of history is offered (not commits that came in through a merge), up to 500 commits. If the
  branch moved since the screen opened (a commit, a pull), starting is refused: cancel and start again.

Back to the [feature index](../../FEATURES.md).
