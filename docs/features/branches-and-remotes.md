# Branches and remotes (left panel)

- **Local branches**: alphabetical list with the current branch highlighted and `↑n` / `↓n`
  when ahead of or behind the upstream. Double-click a branch to check it out (safe
  checkout: refused if uncommitted changes would be overwritten).
- **Branch** button: create a branch from the current commit and check it out; the name is
  validated and uncommitted changes carry over.
  The **⋯** button in the Local branches headline has *New branch…*, which opens the same name form inline, and *Delete synced branches (n)*: it deletes
  every local branch that has an upstream and points at exactly the same commit as it (as of the last fetch), so nothing is lost: the commits stay on the remote. The
  checked-out branch is never deleted, and branches that are ahead, behind or have no upstream are kept. A confirmation lists the branches first, and **Undo** brings them back.
- **Clean up merged…** (same menu, and in the **⋯** menu of the Remotes headline): deletes the branches whose work is already in the main branch.
  - *Main* is `main`, else `master` (and, for the remotes, the remote's own `main`/`master`, found through `<remote>/HEAD`). A branch is merged when its tip is
    an ancestor of main's tip, so nothing on it is lost. The checked-out branch, `main`, `master`, `develop`, `dev` and `trunk`, and branches that still sit exactly on
    main's tip (they may be new, with no work yet) are never listed.
  - **Local:** local branches merged into the local main or into any remote's main. A confirmation lists them; one **Undo** brings them all back.
  - **Remote:** each remote branch merged into *its own* remote's main, except the upstream of the checked-out branch. They are deleted **on the server** with
    `git push --delete` (your credentials apply), after a confirmation that says so. This can't be undone from the app. Fetch first so the lists are current.
- Hovering a branch in the branch lists shows a pointer cursor.
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
- **Context menus** on branches: *Rename branch* (also for the checked-out branch: HEAD follows it, your files and its upstream settings are untouched) and *Delete branch* (disabled for the
  checked-out branch), and on remote branches *Check out* (also a double-click on the branch: creates a local branch of the same name that tracks it, or switches to it if it already does; refuses when a same-named local branch tracks something else, and keeps your local changes safe like any checkout), *Rename remote branch* and *Delete remote branch* (both disabled for
  the branch the checked-out branch tracks). Renaming edits the name inline (Enter confirms,
  Esc cancels). A remote rename asks first, pushes the new name and deletes the old one in one
  atomic push (refused if somebody pushed to it since your last fetch), and points local
  branches that tracked it at the new name. Deleting a branch also asks first, and warns when
  commits would exist nowhere else; remote deletion runs `git push origin --delete`.

Back to the [feature index](../../FEATURES.md).
