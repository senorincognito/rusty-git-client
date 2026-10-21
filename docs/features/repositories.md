# Repositories and start screen

- Open any folder through the native picker. The repo is found by searching upward, so
  picking a subfolder works.
- Recent repositories list (up to 20, newest first), stored in the app data directory.
  Entries whose folder no longer exists are dropped; individual entries can be removed.
- The start screen has two panels side by side (stacked in a narrow window). On the left, the **Recent** repositories. On the right, **Repositories**: choose a
  folder that contains your repositories (**Choose folder…**) and every repository inside it is listed for quick switching, with its name, the folders in between when
  it is nested, its current branch (or `detached`) and how long ago it was last committed to. The folder is remembered between sessions. Repositories up to three
  levels down are found (at most 500; hidden folders and `node_modules` are skipped, and nothing inside a repository is searched). Type in the filter field to narrow the
  list, click a repository to open it, **↻** to scan again, **Change folder…** to pick another one, **Forget folder** to stop listing it (nothing is deleted).
- Shows the current branch, or `detached @ <sha>` for a detached HEAD, and handles
  freshly initialised repos with no commits.

Back to the [feature index](../../FEATURES.md).
