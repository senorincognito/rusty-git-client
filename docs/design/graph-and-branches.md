# Graph, branches and search: design notes

Contributor notes moved out of CLAUDE.md: how it is built and why. What the user sees is described in `docs/features/`. Covers the start screen, graph highlighting and checkout, branch rename and clean-up, the WIP row and commit search.

- **Start screen** (`features/welcome/Welcome`): two panels (CSS grid; stacked under 800px): Recent on the left; on the right the repositories found in one remembered folder (`repo_folder.json`), filtered with `matchesFilter` from the sidebar, opened with the same `openRepo` (so they also become recent). One folder only; several are a possible extension. Not live: a rescan (↻) or reopening the screen refreshes it. Cards show the branch and last-commit age but no change status (opening each repository to count changes would be slow for hundreds).

- **Graph highlighting** (`Graph.tsx` `isCurrent`, `Graph.scss`): the row of the commit HEAD points at (a ref label with `isHead`, also a detached HEAD) gets `.current` (accent bar via inset
  box-shadow, faint tint placed *before* hover/selected so they win, bold subject) and a ring around its node; rows with `!onHead` (not reachable from HEAD, not WIP/stash) get `.off` (text at 60% opacity). The accent colour is
  hard-coded as `rgba(63, 167, 160, …)` for the tints because the SCSS token is a CSS variable.
- **Checkout from the graph** (`Graph`: `checkoutTargets`, `checkoutItems`, `doubleClickTarget`; `RepoView.checkoutRef`): a row whose `refs` include a `branch` or `remote` label that is not `isHead` (so never the checked-out branch
  or a detached HEAD) gets *Check out <name>* first in its menu (a submenu if several) and a double-click does it (local branch preferred, else the single remote one). Local = `checkout_local_branch`;
  remote = `checkout_remote_branch` with the label split at its first `/` (remote names with a slash would fail with "not found"). Stashes and the WIP row offer nothing.
- **Remote branch checkout** (`checkout_remote_branch`, double-click or first item of its menu): creates local `<name>` at the remote tip with
  upstream `<remote>/<name>` and checks it out safely (the new branch is removed again if the checkout fails); an existing local
  branch is reused only if it already tracks that remote branch, otherwise it errors rather than taking it over.
- **Branch rename** (inline editor in the sidebar) works for every local branch, the checked-out one included (libgit2's `Branch::rename` moves HEAD and the `branch.<n>.*` config with it; the files are untouched; the
  remote-tracking branch it follows keeps its name), but is not allowed for the branch the checked-out branch tracks (remote). Remote rename = one atomic push of the new name plus
  deletion of the old one, guarded by a lease; local branches that tracked it are repointed.

- **Uncommitted changes** are a pseudo commit (`WIP_ID` "WIP", `isWip`) inserted by `build_graph` as the first
  row when HEAD has a commit and `status_of` reports changes: a dashed lane (`Edge.dashed`) leads down to HEAD's
  commit, so it lands in that commit's lane. Clicking it calls `closeCommit()` (right panel returns to `Changes`);
  it counts as selected whenever no commit is selected. It has no context menu. File edits aren't watched, so
  `Graph` also refetches on window focus.

- **Commit search** (title bar *Search*, the far right, or Ctrl/Cmd+F; `RepoView` owns `searchOpen` and `searchFocus`, a counter that refocuses an open bar): `Graph` renders `SearchBar` above its scroller. The bar debounces (200 ms), re-searches on `refreshKey`, reports hits (`onHits`, highlighted as `.match` rows) and steps with Enter / Shift+Enter from the selected hit (wrapping). Stepping selects the commit through `onSelectCommit` and sets `reveal`; because the graph is paged, `Graph` then loads through the hit's `index` in one request (`loadMore`) and centres it. Hits are not auto-selected while typing (that would keep replacing the right panel). Opening the search closes a diff or file history covering the graph; it is off during an interactive rebase.

- **Clean up merged** (`sidebar/cleanup.rs`; item in the "⋯" menus of Local branches and Remotes): merged = the tip is a *strict* ancestor of a main tip (`graph_descendant_of(base, tip)`), so branches sitting exactly on main are excluded on purpose (new branches). Main = local `main`/`master`; per remote `<remote>/HEAD`'s target, else `main`/`master`. Local candidates are checked against the local main *and* every remote main; remote candidates only against their own remote's main (that is what the server has). Never offered: the checked-out branch, `PROTECTED` names (main, master, develop, dev, trunk) and the base's own name, and a remote branch that is the checked-out branch's upstream. The UI asks the backend for the list, shows it in a confirmation, and the delete commands recompute the list and only delete the intersection. Remote deletion runs one `push --delete` per remote with the system git and is outside the undo journal; the confirmation says so.
- **Delete synced branches** (Local branches "⋯" menu, `delete_synced_branches`): "synced" = has an upstream and the local tip equals the upstream tip (`ahead == 0 && behind == 0` in `BranchInfo`, so relative to the last fetch), never the checked-out branch. The UI lists them in the confirmation; the backend re-checks each name before deleting and silently skips any that no longer qualify. One undo step (`Kind::Keep`).
