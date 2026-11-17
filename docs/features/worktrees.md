# Worktrees

A worktree is a second working directory of the same repository with its own branch checked out, so you can work on two branches (or let two tools or agents work) at once without stashing or switching.

- The **Worktrees** section in the Branches tab of the left panel lists the main working directory and every linked one, with the branch (or "detached at …") and the folder name. The one shown in the current tab is marked.
- **Click a worktree** to open it in its own tab; everything in the app (graph, changes, terminal, ...) then works on that folder. Opening the same one again just activates its tab.
- **New worktree…** (the **⋯** menu of the headline): type a branch name. An existing branch is checked out in the new worktree; a new name creates the branch from the current commit. The folder is `<repo>.worktrees/<branch>` next to the repository (slashes in the branch name become dashes), so it never shows up as untracked files inside it. git refuses a branch that another worktree already has checked out.
- **Remove worktree…** (right-click): deletes the worktree's folder after a confirmation; the branch and its commits stay. If the folder has modified or untracked files, git's refusal is shown and you can choose to remove it anyway (those changes are lost). The main working directory and the worktree shown in the current tab can't be removed (the menu item says why); a removed worktree's tab closes.
- **Prune missing worktrees** (**⋯** menu): forgets worktrees whose folder was deleted by hand (they are shown dimmed with a "missing" badge).
- A branch that another worktree has checked out is marked with a *worktree* badge in the Local branches list (the tooltip names the folder) and with an outlined chip in the commit graph. Checking it out, renaming or deleting it is refused with a message naming that worktree (Rename and Delete are disabled in the menu, and the clean-up commands skip it), because a branch can be checked out in one working directory only.
- A number at the right of a worktree row is how many files have uncommitted changes in it. It is looked up after the list shows, and again when the window regains focus (edits in other folders aren't watched).
- The filter field narrows the list by branch or folder name. Worktree changes are not part of Undo.

Back to the [feature index](../../FEATURES.md).
