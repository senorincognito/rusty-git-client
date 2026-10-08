# Features

What Rusty Git Client can do today. For setting it up and building it, see the [README](README.md).

## Repositories
- Open any folder through the native picker. The repo is found by searching upward, so
  picking a subfolder works.
- Recent repositories list (up to 20, newest first), stored in the app data directory.
  Entries whose folder no longer exists are dropped; individual entries can be removed.
- Shows the current branch, or `detached @ <sha>` for a detached HEAD, and handles
  freshly initialised repos with no commits.

## Commit graph
- All local branches, remote branches and tags, plus a detached HEAD, newest first.
  Stash and notes refs are left out.
- Coloured lane layout computed in Rust: branches, merges and joins are drawn as curved
  lines per row.
- Ref chips on each commit (current branch highlighted; branches, remotes and tags
  styled separately).
- Columns for message, author, date and short hash.
- Virtualised rendering, with history loaded in pages of 1000 as you scroll.
- While there are uncommitted changes, a dashed "N file changes in working directory" row sits on top
  of the graph, joined to the commit you are on. Clicking it shows those changes (the staging panel) in
  the right panel; it is highlighted whenever no commit is selected. The row updates when you come back
  to the window after editing files elsewhere.
- Click a row to select it.

## Creating commits
- **Changes panel** with Unstaged and Staged file lists and status badges
  (A added, M modified, D deleted, T type change, ! conflicted).
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

## Commit details
- Click a commit in the graph: the right panel switches to that commit's message, author, date,
  parents and the files it changed (added, modified, deleted, renamed with their old path).
  Merge commits are shown against their first parent.
- Click a changed file: the centre area shows the file with the commit's changes marked in
  place (added lines in green with `+`, removed lines in red with `-`, old and new line
  numbers). A **Full file** switch toggles between the whole file and just the changed hunks
  with three lines of context. Binary and very large files are not previewed, and diffs are
  capped at 20,000 lines. In the full-file view, green and red marks along the scrollbar show where the added and removed lines are
  (the scrollbar itself still works as usual). The ▲ / ▼ buttons in the diff header jump to the previous / next change (hunk) and show
  which one you are at ("3 / 7"); a button is greyed out when there is nothing further in that direction. **Back to graph** returns to the commit graph exactly where you left it.
- Click a file in the staging panel (Unstaged or Staged): the centre area shows its diff the same
  way. Unstaged compares the index with the file on disk (untracked files show as all added),
  Staged compares HEAD with the index. It refreshes when you edit, stage or return to the window,
  and keeps its scroll position.
- Every run of changed lines is a **hunk** with its own heading ("Hunk 2 of 5 · +3 -1"), in all diff views. On
  the *unstaged* diff of a tracked file each heading has **Stage hunk** (puts just that hunk into the staging
  area) and **Discard hunk** (removes it from the file after a confirmation; cannot be undone); on the
  *staged* diff each heading has **Unstage hunk** (takes just that hunk back out of the staging area, the
  file on disk is untouched; a newly added file is removed from the index, a staged deletion comes back). Line endings,
  including CRLF files and a missing final newline, are preserved, and a hunk whose file changed since the diff
  was shown is refused instead of applied to the wrong lines. New (untracked) files and binary files have no
  hunk buttons: stage the whole file from the list.
- While you have uncommitted changes, a notice at the top of the panel says how many files
  changed in the working directory, with a **View changes** button that closes the commit view
  and returns to the staging panel (your draft commit message is kept). The × does the same.

## Stashes
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
  and the stash is kept. Applying without removing is not available yet.
- **Delete stash** (right-click the stash in the Stashes list) removes it without applying it, after a confirmation.
  It works with uncommitted changes in the working directory; the stash's changes are lost.

## Commit graph context menu
- Right-click a commit: **Rename commit** opens an editor in the right panel with the full
  message and **Update** / **Cancel** buttons (Esc cancels, Ctrl+Enter updates). Only
  commits on the current branch can be renamed.
- Update rewrites the commit's message and rebuilds every later commit on the branch on top
  of it (same files, authors and dates, new ids), then moves the branch. Files and the index
  are untouched, so there can be no conflicts. Other branches keep their old history. The
  panel warns when later commits are rewritten and when the commit is already pushed (a
  force push is then needed).

## Interactive rebase
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

## Merging a branch
- Right-click a **local branch** (*Merge <branch> into <current>*) or a **remote branch** (*Merge <remote>/<branch> into the current branch*) to merge it into
  the checked-out branch. A dialog asks first. A merge commit is created with Git's default message, or the branch simply fast-forwards when it can; uncommitted
  changes are set aside and restored. If the branch is already contained you are told so. Conflicts are not resolved in the app yet: the merge is
  cancelled, the conflicting files are named and nothing changes. Not available on a detached HEAD.

## File history
- Right-click a changed file in the staging panel (staged or unstaged) and choose **File history**. The centre area then lists every commit that
  changed that file, newest first (up to 500), with a badge for what the commit did to it (new, modified, renamed, deleted), the short id, message,
  author and date. The history follows the file across renames. Click a commit to see exactly what it changed in the file, in the usual diff view
  (**Back to file history** or Esc returns to the list at the same scroll position; Esc in the list returns to the graph). A file that is new and
  not committed yet has no history, so the item is greyed out.
- The same **File history** item is in the right-click menu of a file in a commit's file list (right panel). The history then starts at exactly that commit and
  leaves out every later change to the file (the title says "File history up to <id>"); the file is followed back across renames from the name it had in that commit.
  Not offered for stash files, and not while the interactive rebase screen is open.

## Fast-forward
- Right-click a **local branch** and choose **Fast-forward <current> to <branch>**, or right-click a **commit** in the graph and choose
  **Fast-forward to this commit**: the checked-out branch moves forward to it, like `git merge --ff-only`, and your files are updated.
  It only works when the target is ahead of the checked-out branch. If it already contains the commit, the histories have diverged
  (the message says by how much) or uncommitted changes are in the way, an error explains why and nothing changes. No confirmation:
  nothing is lost.

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

## Branches and remotes (left panel)
- **Local branches**: alphabetical list with the current branch highlighted and `↑n` / `↓n`
  when ahead of or behind the upstream. Double-click a branch to check it out (safe
  checkout: refused if uncommitted changes would be overwritten).
- **Branch** button: create a branch from the current commit and check it out; the name is
  validated and uncommitted changes carry over.
  The **⋯** button in the Local branches headline has *New branch…*, which opens the same name form inline.
- **Resizable sections**: drag the bottom edge of a left-panel section (Local branches, Remotes) to set its height, and the bar
  between the Unstaged and Staged lists in the right panel to share their heights. Arrow up/down on a focused bar nudge it (Shift =
  bigger steps), double-click resets. The sizes are remembered. A section can never be dragged so far that another one loses its headline.
- **Long lists**: the three sections share the panel's height, and a long list scrolls inside its own section, so the Local
  branches, Remotes and Stashes headlines stay visible whatever the number of branches (collapse a section to give the
  others the room).
- **Folders**: branch names with a `/` are grouped like folders, in local branches and under each remote (`feature/login`
  is `login` inside a `feature` folder, with a branch count). Click a folder to close or open it; a filter opens them all.
- **Filter** (the field at the very top of the left panel): type part of a name to narrow the local branches, remote
  branches (matched as `origin/name`, or all of a remote's branches when the remote's name matches) and stashes (by message
  or `stash@{n}`) at once. Several words must all match, case does not matter; Esc or the × clears it.
- **Remotes**: every remote of the repository with its URL and remote branches. The **⋯** button in the section's headline opens a menu with
  *Add remote…* (a name and URL form; shown right away when there is none yet) and, with several remotes, a *Target
  remote* group to pick the target. Right-click a remote's header for *Set as target*, *Edit URL* (Enter
  saves, Esc cancels; the remote branches stay until the next fetch) and *Remove remote*. The **target** remote (marked
  when there are several; the one you chose, else `origin`, else the first) is where **Push** publishes a branch that has no upstream yet;
  branches that already track something keep pushing and pulling there. Removing a remote asks first and only changes
  this repository's settings (its remote-tracking branches go, local branches that tracked it lose their upstream; nothing on the server is touched). A yellow ⚠ beside a remote's name means the last fetch
  failed (hover it for git's message); it disappears after the next successful fetch, pull or background fetch.
- **Context menus** on branches: *Rename branch* and *Delete branch* (both disabled for the
  checked-out branch), and on remote branches *Check out* (also a double-click on the branch: creates a local branch of the same name that tracks it, or switches to it if it already does; refuses when a same-named local branch tracks something else, and keeps your local changes safe like any checkout), *Rename remote branch* and *Delete remote branch* (both disabled for
  the branch the checked-out branch tracks). Renaming edits the name inline (Enter confirms,
  Esc cancels). A remote rename asks first, pushes the new name and deletes the old one in one
  atomic push (refused if somebody pushed to it since your last fetch), and points local
  branches that tracked it at the new name. Deleting a branch also asks first, and warns when
  commits would exist nowhere else; remote deletion runs `git push origin --delete`.

## Fetch, pull and push
- Title-bar buttons run the system `git`, so your credential helper and SSH setup apply.
- Fetch updates all remotes (with prune). Pull fast-forwards, so it never creates a surprise
  merge. When the branches have diverged, a dialog lists the commits on each side and offers
  **Merge** (the default: keeps every commit, adds a merge commit) or **Rebase** (replays your
  commits on top of the upstream, giving them new ids). Uncommitted changes are set aside and
  restored. If there are conflicts, the pull is cancelled with the file names and the repository is
  left exactly as it was (resolve in the terminal for now). Push publishes a new branch to `origin` and sets its upstream.
- **Sign-in prompt** (macOS): when a fetch, pull or push needs a username, password, token or SSH passphrase and no credential
  helper supplies it, a dialog asks for it (for GitHub and similar over HTTPS enter a personal access token as the password).
  Cancel aborts the operation. If Git has a credential helper (e.g. the macOS keychain) it stores the login afterwards. Windows
  uses Git Credential Manager's own window. Auto-fetch never asks; it pauses on missing credentials as before.
- **Push on a diverged branch**: when your branch and its upstream each have commits the other lacks, Push does not just fail. A dialog lists both
  sides and offers **Force push** (with lease; the remote-only commits are discarded there) or **Cancel**, which is the default and changes nothing. To keep the
  remote commits, pull (merge or rebase) first. The check uses the last fetched state; fetch first if you suspect the remote moved.
- **Auto-fetch** (on by default, every 3 minutes): fetches in the background while the window
  is focused, and right away when you come back to a stale repo. It never overlaps another
  git operation, stays silent (a small spinner shows while it runs), backs off when the
  remote is unreachable, and pauses instead of retrying when credentials are needed. Switch it
  off or pick 1 / 3 / 5 / 10 minutes from the ▾ next to Fetch (or right-click Fetch).
- **Pull options**: click the small ▾ beside Pull (or right-click it) to pull with an explicit
  strategy, **Pull (merge)** or **Pull (rebase)**, without the diverged-branches dialog.
- **Force push**: right-click the Push button, or click the small ▾ beside it. It asks for
  confirmation first (and says how many remote commits will be discarded), then runs
  `git push --force-with-lease`, which is refused if the remote moved since your last fetch.
  Use it after amending or renaming a commit that was already pushed.
- Buttons show `↓n` / `↑n` counts and are disabled with a tooltip when they can't work.

## Terminal
- Toggle a real terminal (PowerShell on Windows, your login shell on macOS) in the
  repository folder with the **>_ Terminal** button or the Ctrl + Backquote shortcut. Resizable, keeps its
  session while hidden, restarts when you switch repositories.
- Anything you run there (commits, checkouts, ...) shows up in the UI through live reload.

## Keyboard shortcuts
| Key | Action |
| --- | --- |
| `Esc` | Close the diff in the centre (same as **Back**); also closes menus and dialogs and cancels inline editors |
| `Ctrl` + `` ` `` | Show or hide the terminal |
| `Ctrl`/`Cmd` + `Enter` | Commit (staging panel) or Update (rename dialog) |
| `Enter` / `Space` | Open the focused file row; `Enter` confirms an inline branch rename |
| `←` / `→` on a panel's resize handle | Resize it (hold `Shift` for bigger steps) |
| `↑` / `↓` while a file's diff is open | Open the file above / below in the right panel's list (a commit's files, or unstaged then staged files) |
| `↑` / `↓` in the commit graph | Select the commit above / below the selected one (the uncommitted-changes row counts as selected while no commit is) |
| `↑` / `↓` in the interactive rebase | Select the commit above / below (with nothing selected: `↓` selects the top commit, `↑` the bottom one) |
| `Ctrl`/`Cmd` + `↑` / `↓` in the interactive rebase | Move the selected commit up / down (`Cmd` on macOS, `Ctrl` elsewhere) |

Esc leaves text fields and the terminal alone, so it never interferes with typing.

## Not yet implemented

- Tags in the sidebar
- Applying a stash without removing it
- Merge and rebase as standalone actions, revert commit
- Line-level (single line) staging and unstaging
- A conflict-resolution UI (a pull that conflicts is cancelled and the repository left untouched)
- Syntax highlighting and intra-line diff highlighting, side-by-side diff
- Multiple remotes work, but only the target remote is used to publish new branches
