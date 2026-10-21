# Features

What Rusty Git Client can do today. For setting it up and building it, see the [README](README.md).
The details live in one file per area under [docs/features/](docs/features/).

## Areas
- [Repositories and start screen](docs/features/repositories.md): Opening repositories, the recent list and the folder of repositories on the start screen.
- [Repository tabs](docs/features/tabs.md): Several repositories open at once, switched and closed like browser tabs; background tabs do not auto-fetch.
- [Commit graph](docs/features/commit-graph.md): The lane graph, its highlighting, the working-directory row and the right-click menu of a commit.
- [Creating commits](docs/features/commits.md): The Changes panel, committing and amending, and the right-click menu of a changed file.
- [Commit details, diffs and file history](docs/features/commit-details.md): A commit's files, the diff views, hunk and line staging, and the history of one file.
- [Stashes](docs/features/stashes.md): Creating, applying, popping and deleting stashes.
- [Interactive rebase](docs/features/interactive-rebase.md): Reword, squash, drop and reorder commits on the current branch.
- [Merge, rebase, fast-forward, drop and reset](docs/features/history-operations.md): Single-step operations that move or rewrite the checked-out branch.
- [Undo and Redo](docs/features/undo-redo.md): What the Undo / Redo buttons take back and when they refuse.
- [Branches and remotes (left panel)](docs/features/branches-and-remotes.md): Local branches, clean-up, remotes, the filter and branch / remote context menus.
- [Fetch, pull and push](docs/features/sync.md): The sync buttons, diverged branches, force push, auto-fetch and sign-in prompts.
- [Terminal](docs/features/terminal.md): The built-in terminal panel.

## Keyboard shortcuts
| Key | Action |
| --- | --- |
| `Esc` | Close the diff in the centre (same as **Back**); also closes menus and dialogs and cancels inline editors |
| `Ctrl` + `` ` `` | Show or hide the terminal |
| `Ctrl`/`Cmd` + `Enter` | Commit (staging panel) or Update (rename dialog) |
| `Ctrl`/`Cmd` + `Z` / `Ctrl`/`Cmd` + `Shift` + `Z` (or `Ctrl` + `Y`) | Undo / Redo the last action (not inside text fields or the terminal) |
| `Enter` / `Space` | Open the focused file row; `Enter` confirms an inline branch rename |
| `←` / `→` on a panel's resize handle | Resize it (hold `Shift` for bigger steps) |
| `↑` / `↓` while a file's diff is open | Open the file above / below in the right panel's list (a commit's files, or unstaged then staged files) |
| `↑` / `↓` in the commit graph | Select the commit above / below the selected one (the uncommitted-changes row counts as selected while no commit is) |
| `↑` / `↓` in the interactive rebase | Select the commit above / below (with nothing selected: `↓` selects the top commit, `↑` the bottom one) |
| `Ctrl`/`Cmd` + `↑` / `↓` in the interactive rebase | Move the selected commit up / down (`Cmd` on macOS, `Ctrl` elsewhere) |

Esc leaves text fields and the terminal alone, so it never interferes with typing.

## Not yet implemented

- Tags in the sidebar
- Merge and rebase as standalone actions, revert commit
- Unstaging a single line (staging and discarding one are built)
- A conflict-resolution UI (a pull that conflicts is cancelled and the repository left untouched)
- Syntax highlighting and intra-line diff highlighting, side-by-side diff
- Multiple remotes work, but only the target remote is used to publish new branches
