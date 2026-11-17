# CLAUDE.md

Guidance for working on **Rusty Git Client**: a desktop Git GUI in the spirit of GitKraken, built with
Tauri 2 (Rust backend, `git2`/libgit2 plus the system `git`) and React + TypeScript + Vite.
Targets Windows and macOS. `FEATURES.md` is the index of the user-facing features plus the shortcuts table, with the details in one file per area under `docs/features/` (update the matching file with every feature; add a file and an index line for a new area, split a file once it passes ~100 lines) and `README.md` covers requirements, running
and building; this file is for contributors.

**Keep this file current.** The author has given standing permission to update it without asking: when work
adds a module or a rule, settles a product decision, or uncovers a quirk (library, git, Windows, tooling),
record it in the right place, briefly and with the *why*: rules and quirks here, a feature's decisions and implementation details in its file under
`docs/design/` (see "Design notes"). Edit existing entries rather than duplicating them, and move finished items off the "Not yet implemented" list in `FEATURES.md`.

## Commands

```sh
npm install
npm run tauri dev                                    # run the app (hot reload; a Rust change restarts it)
npm run build:run                                    # release build without installers, then start it (--skip-build: just start)
npm run lint                                         # ESLint (src/ only)
npx tsc --noEmit                                     # typecheck the frontend
npx vite build                                       # bundle (also proves the "@/" alias resolves)
cargo test --lib --manifest-path src-tauri/Cargo.toml   # all backend tests (needs system `git` on PATH)
cargo check --manifest-path src-tauri/Cargo.toml     # keep it warning-free
cargo fmt --manifest-path src-tauri/Cargo.toml       # rustfmt (src-tauri/rustfmt.toml: width 120); CI runs --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings   # CI fails on any warning
```

Before calling work done: `cargo test --lib`, `cargo fmt --check`, `cargo clippy`, `npm run lint`, `npx tsc --noEmit` and `npx vite build` (which also compiles all the
SCSS: a malformed rule is a build error). There is no frontend test runner and the UI can't be driven from here, so
say plainly that UI behaviour is untested in the running app.

## Architecture

**Data flow.** The frontend calls typed wrappers in `src/api/*`, which `invoke()` Tauri commands in
`src-tauri/src/*`. `repo/watch.rs` watches `.git` (HEAD, index, `refs/`) and emits `repo-changed`;
`RepoView` listens, bumps `graphKey`, and every panel reloads from it. So any change made anywhere
(terminal, editor, another tool) shows up without a manual refresh.

### Backend (`src-tauri/src/`)

The folders follow the frontend's `src/features/` (one folder per area of the UI; `src/api/` has one wrapper file per
backend file). A folder's `mod.rs` holds what the area as a whole needs; the other files are its parts. Every submodule
is `pub mod` and `lib.rs` registers commands by full path (`history::rebase::apply_rebase_cmd`): `generate_handler!` needs
the module path to be visible from the crate root.

| Folder / file | Frontend feature | Commands / role |
| --- | --- | --- |
| `repo/mod.rs` | `features/welcome`, `features/repo` | `open_repo`, recent repos (JSON in the app data dir) |
| `repo/folders.rs` | `features/welcome` | `get_repo_folder` / `set_repo_folder` (remembered start-screen folder), `scan_repo_folder`: repositories ≤ 3 levels down (max 500; hidden folders and `node_modules` skipped; symlinks not followed; a repository is listed, never searched) |
| `repo/watch.rs` | `features/repo` | the `.git` watcher (`watch_repo`, `unwatch_repo`) |
| `graph/mod.rs` | `features/graph` | `get_graph(path, skip, limit)`: revwalk over all refs, lane layout computed in Rust, `on_head` flag per row. Paged: the topological order (libgit2 can't yield it lazily, it reads all history first) and the layout at the end of the last page are cached per repo (`CACHE`, keyed by the sorted tips + HEAD, so any ref move recomputes); a page with `skip` equal to where the last one ended resumes from the saved lanes, anything else lays out from the top. `limit` counts commits, the WIP row is extra and only on `skip == 0`. The frontend appends pages (1000 at a time, near the end of the list) and a reload asks for everything loaded so far in one request |
| `graph/fast_forward.rs` | graph + branch context menus | `fast_forward_cmd`: move the checked-out branch forward to a commit id or full ref name |
| `graph/merge.rs` | branch context menus | `merge_branch_cmd`: merge a branch (full ref name) into the checked-out one via system git; returns `upToDate`/`fastForward`/`merged` |
| `graph/pick.rs` | graph context menu | `cherry_pick_cmd`, `revert_commit_cmd`: system git `cherry-pick`/`revert --no-edit` (`-m 1` for merge commits); a conflict is aborted with `<op> --abort` (`cancel_unfinished` only knows merge/rebase) and the files named; `Kind::Switch`. "Checkout this commit" is `checkout_commit_cmd` in `sidebar/branches.rs` (safe checkout + detached HEAD); "Copy commit SHA" is frontend-only (`navigator.clipboard`) |
| `graph/rebase_onto.rs` | local branch context menu | `get_rebase_onto_info` (commits replayed, pushed, merges for the confirmation), `rebase_onto_cmd`: `git rebase --autostash <ref>` of the checked-out branch onto a branch (full ref name); returns `upToDate`/`fastForward`/`rebased` |
| `graph/search.rs` | `features/graph/SearchBar` | `search_commits(path, query)`: full messages (case-insensitive substring) and id prefixes (≥ 4 hex digits) over the graph's own topological order (`take_order`, shared with `build_graph` and its cache), so each hit carries its row index; max 1000 hits (`truncated`) |
| `graph/tags.rs` | graph context menu | `create_tag_cmd` (lightweight, or annotated when a message is given; existing names refused, never moved), `delete_tag_cmd` (local only), `get_tags` (every tag with its peeled commit, annotated flag and message, newest first; feeds the sidebar's Tags tab). Plain `git2`, outside the undo journal (snapshots don't cover tags). Tags are already `kind: "tag"` `RefLabel`s in the graph; `search.rs` also matches tag names via `inputs.labels`; UI: `features/graph/TagDialog`, `RepoView.tagging`/`deleteTagByName`, `features/sidebar/Tags` |
| `graph/reset.rs` | graph context menu | `get_reset_info` (what a reset would remove/add, pushed count, uncommitted files), `reset_to_commit` (soft/mixed/hard) |
| `history/mod.rs` | graph context menu | shared by the rewrites below: `is_pushed`, `rewrite_plan`, `rebuild_with_messages`, `move_head_to`, `blocking` |
| `history/rename.rs` | `features/rename` | `get_rename_info`, `rename_commit_message` (rewrites the commit and its descendants) |
| `history/rebase.rs` | `features/rebase` | `get_rebase_plan`, `apply_rebase_cmd` (interactive rebase: pick/reword/squash/drop/reorder), `replay_group` |
| `history/drop.rs` | graph context menu | `get_drop_info`, `drop_latest_commit` (drop one commit) |
| `history/test_support.rs` | tests only | small repositories with real commits for the history tests |
| `changes/mod.rs` | `features/changes` | status, stage/unstage, `discard_paths` (whole-file discard), `create_commit` (new or `amend`), `get_head_commit`. `commit_staged`/`amend_head` validate with git2, then `git_commit` runs system `git commit -F <tempfile> [--amend]` so hooks, signing and templates apply (libgit2's `commit()` never runs hooks). The history rewrites (rename, drop, interactive rebase) still build commits with libgit2 and so skip hooks, `post-rewrite` included |
| `changes/conflicts.rs` | `features/conflicts`, `features/changes` | merge conflicts left in place: `get_merge_state`, `get_conflict` (working file parsed into text/conflict segments, `parse_markers`), `resolve_conflict_side_cmd` (system git `checkout --ours/--theirs` + `add`, or `rm`), `save_resolution_cmd`, `abort_merge_cmd`; `stage` refuses a conflicted file that still has markers |
| `changes/hunks.rs` | diff view | `stage_hunk`, `discard_hunk`, `unstage_hunk`: apply one hunk of a file's staged/unstaged changes (see `docs/design/changes-and-diffs.md`) |
| `commit/file_history.rs` | `features/commit/FileHistory` | `get_file_history`: `git log --follow --name-status` for one file (≤ 500 commits), parsed into id/author/time/status/path-at-that-commit/oldPath |
| `commit/mod.rs` | `features/commit` | all diff rendering: `get_commit_detail` (files of a commit, renames), `get_file_diff` (a commit's file), `get_working_diff` (staged/unstaged file); shared `diff_options` + `render_diff` |
| `files/mod.rs` | `features/sidebar/Files`, `features/files/FileViewer` | `get_files` (`git ls-files -z --cached --others --exclude-standard`, so tracked plus untracked-not-ignored; entries missing on disk and directories such as submodules dropped, sorted), `create_file_cmd` / `save_file_cmd` / `delete_file_cmd` (plain `std::fs`, outside the undo journal; `relative()` refuses `..`, absolute paths and anything under `.git`; a write checks the nearest existing ancestor is inside the repo *before* `create_dir_all`, so a symlinked folder can't lead out; delete refuses folders), `files/blame.rs` `get_line_blame` (`git blame --porcelain -L n,n` on the file as it is on disk; `None` for uncommitted lines, untracked files and lines past the end), `get_file_content` (working-directory file as `text`/`binary` (NUL in the first 8000 bytes)/`tooLarge` (> 2 MB); the path must be relative and `canonicalize`d inside the repo, so `..` and symlinks out are refused) |
| `sidebar/branches.rs` | `features/sidebar` | list, create+checkout, checkout, delete, rename (local) |
| `sidebar/cleanup.rs` | `features/sidebar` | "Clean up merged": `get_merged_branches(path, remote)`, `delete_merged_local` (one undo step), `delete_merged_remote` (`git push <remote> --delete`, not undoable) |
| `sidebar/remotes.rs` | `features/sidebar` | `get_remotes` (every remote with branches, `isTarget`, tracking count), `add_remote_cmd`, `set_remote_url_cmd`, `delete_remote_cmd`, `set_target_remote`; delete/rename remote branches take a `remote` argument |
| `sidebar/worktrees.rs` | `features/sidebar/Worktrees` | `get_worktrees` (`git worktree list --porcelain`, main first), `add_worktree_cmd` (existing branch as is, else `-b` from HEAD; folder `<repo>.worktrees/<branch>`), `remove_worktree_cmd` (`force` for dirty ones), `prune_worktrees_cmd`. System git, outside the undo journal. A linked worktree opens as a normal tab; its `.git` is a file, see the watcher note in `docs/design/worktrees.md` |
| `sidebar/stash.rs` | `features/sidebar`, `features/changes` | `get_stashes`, `create_stash` (stashes everything incl. untracked), `stash_paths_cmd` (selected files, via system git), `pop_stash_cmd`, `apply_stash_cmd`, `drop_stash_cmd`, `drop_all_stashes_cmd`; helpers `stash_index_of`, `untracked_tree` |
| `toolbar/sync.rs` | `features/toolbar` | fetch / pull / push / force push / auto-fetch / diverged pull; `run_git`, `run_git_with` |
| `auth/mod.rs` | `features/auth` | credential prompts: `GIT_ASKPASS`/`SSH_ASKPASS` script, loopback socket, `credentials-request` event, `answer_credentials` |
| `undo/mod.rs` | `features/toolbar/UndoButtons` | the undo journal: `recorded(path, label, kind, || op)` wraps a command, `get_undo_state`, `undo_cmd`, `redo_cmd` |
| `gitlog/mod.rs` | `features/gitlog` | the command log: `run_git_inner` (the only place that spawns git) calls `begin`/`finish`; the last 500 calls are kept in memory and published as `git-log` events (twice per call, same `id`); `get_git_log`, `clear_git_log`. libgit2 operations run no command and are not logged |
| `terminal/mod.rs` | `features/terminal` | PTY sessions (`portable-pty`), one per working directory, owned by the backend: they outlive the panel (`term_start` attaches and replays the last 256 KB, `term_detach`, `term_stop`, `term_run` types a command into one, `term_list`); see `docs/design/worktrees.md` |
| `terminal/agents.rs` | `features/settings`, `features/sidebar/Worktrees` | `check_agent_commands`: is an agent preset's program on PATH |

Modules reach each other by full path (`crate::toolbar::sync::run_git`, `crate::changes::status_of`,
`crate::sidebar::stash::save_stash`). Tests live in a `#[cfg(test)] mod tests` at the bottom of the file they test.

### Frontend (`src/`, `@/` = `src/`)

```
App.tsx                    Welcome or RepoView
api/                       one typed wrapper file per backend module
features/welcome|repo|graph|changes|commit|rename|rebase|sidebar|toolbar|terminal|settings|gitlog/
components/                generic UI: ContextMenu, Modal, ResizablePanel, Section, FileBadge
hooks/                     useLatestRequest, usePersistentState, useSetting (shared settings), useAutoFetch
i18n/                      en.ts: every user-facing text; index.ts exports it as `t`, plus `fill`
styles/                    global SCSS: tokens, mixins, base, buttons, switch, filelist, panel (see Styles)
*.scss next to components  each component's own styles, imported by that component
```

**Tabs** (`features/tabs/TabBar`, state in `App`): `tabs: RepoInfo[]` plus `active` (a path, or null = start screen, which is also what "+" shows; no tab bar while there are no tabs). Only the active tab's `RepoView` is mounted (`key` = path): that is what keeps background tabs from auto-fetching, and it also keeps their watcher and the window-level key handlers (Esc, Ctrl+Z, arrows...) from firing in several repositories at once. The price is that a tab's UI state (open diff, selection) is gone when you switch away; its terminal session is not (the backend owns it). Opening an already open repository just activates its tab. `RepoView`'s `onRepoChange` must stay a stable callback (its watcher effect depends on it); it replaces the tab's `RepoInfo` (branch label). Open tabs are not persisted across restarts. The old "← Repositories" button is gone: the "+" tab replaces it.

`RepoView` owns the screen state: `graphKey`, `terminalOpen`, `selectedCommit`, `openFile` (a commit's file),
`openWorkingFile` (a staged/unstaged file from the Changes panel), `renaming`.

- **Right panel** (a `ResizablePanel`) shows one of: `RenameCommit` > `CommitDetail` > `Changes`.
  `Changes` stays mounted but hidden so a half-typed commit message survives.
- **Centre** shows the graph, or `FileDiff` on top of it. `FileDiff` takes a `source`: a commit (opened from
  `CommitDetail`), or `staged`/`unstaged` (opened from the Changes lists). Working-tree sources refresh on
  `graphKey` and window focus (file edits aren't watched) without resetting scroll. The graph stays mounted (hidden via
  `position: absolute; visibility: hidden`, NOT `display: none`) to keep its scroll position.
- Graph selection is controlled by `RepoView` (`selectedId`/`onSelectCommit`); right-click does not select.

### Persisted state

`recent_repos.json` and `repo_folder.json` (the start screen's folder of repositories, a JSON string) in the app data dir; `localStorage` keys `sidebarWidth`, `changesWidth`,
`autoFetch.enabled` (default true), `autoFetch.seconds` (default 180), `diff.mode` (`full` | `hunks` | `split`; falls back to the old `diff.fullFile` boolean when unset), `sectionHeight.<local|remotes|stashes>` (px), `changesSplit` (fraction), `terminal.autocomplete` (default true; the Settings dialog), `agents.presets` (the agents of "Start agent", `{ name, command }[]`; the Settings dialog), `gitLog.open` (default false), `gitLog.height` (px).
All of it is keyed by the bundle identifier `net.senorincognito.gitclient` (changed from `com.gitclient.app` in 0.2.x; the old
app data dir and `localStorage` are not migrated, so saved settings and the recent-repos list reset once). Don't change it casually.

## Conventions

- **Commands** are `async` and run blocking work in `tauri::async_runtime::spawn_blocking` (see the
  `blocking` helpers). Register each in `lib.rs`. Serde structs use `rename_all = "camelCase"`; Tauri maps
  JS `oldPath` to Rust `old_path` automatically.
- **git2 vs system git.** Local reads/writes use `git2`. Anything that talks to a remote or needs
  credentials, merges or rebases uses the system `git` via `run_git` / `run_git_with` (user's credential
  helper, SSH agent, config apply; `GIT_TERMINAL_PROMPT=0`, no console window on Windows).
- **Working-tree status is shared** (`getStatus` in `api/changes.ts`): the graph's WIP row aside, `Changes` and every `useWorkingChangeCount` ask on each refresh and a scan of a big tree is slow, so requests in one tick share one scan and a request that arrives while a scan runs gets one follow-up scan (never the running one, which may predate the change it asks about). Always go through `getStatus`, not `invoke("get_status")`.
- **Branch switching has one entry point**: `RepoView.checkoutRef` (graph, sidebar local list, sidebar remote list via `Sidebar`'s `onCheckout`). It owns `switchingTo`; while set, `Sidebar` makes its `nav` `inert` and shows the `.sidebar-lock` overlay (spinner + text), and a second switch is ignored. Failures use the native `showError` dialog. Use the same pattern (state in `RepoView`, prop into the panel) for other slow actions that should lock a panel. `Changes` locks itself with `.changes-lock` while its `run()` (stage, unstage, discard, stash, commit) is busy; the overlay fades in only after 250 ms so fast operations don't flash it (a CSS animation delay, no timer).
- **Activity bars**: every API wrapper imports `invoke` from `api/invoke.ts`, which records the commands in flight per repo `path`. `BusyBar` (sidebar, centre `.center`, right panel) shows a 2px moving bar while `usePanelBusy(path, panel)` is true: a read the panel displays (`READS` map: `get_graph`, `get_status`, ...) or any *change* (every other command with a `path`; `QUIET` lists background/own-indicator ones such as auto-fetch, diffs, search). Shown after 200 ms, held 300 ms, so quick work doesn't flash and the gap between a change and its reload doesn't blink. A new command that only reads must start with `get_`/`count_`/`scan_` (else it counts as a change); a new panel read goes into `READS`. The lock overlays (`switchingTo`, `changes-lock`) remain for actions that must block input.
- **Async UI requests** must ignore out-of-order responses: use `useLatestRequest`
  (`const isCurrent = start(); ...; if (isCurrent()) set(...)`).
- **Sidebar section headlines** can carry a "⋯" menu (`Section`'s `action` prop): Local branches (*New branch…*, which shows
  the same `NewBranchForm` the toolbar's Branch button uses, inline) and Remotes. Keep branch creation in that one form.
- **Sidebar layout**: three tabs (`Sidebar`: Branches = Local + Remotes, Stashes, Tags; the choice is `localStorage` `sidebarTab`). Every tab's `.sidepane` stays mounted and is just `hidden`, so lists keep their data and scroll. `.sidebar` itself does not scroll; inside a pane each open `Section` is a shrinkable flex item (min height 96px) whose
  `.sidebox-body` scrolls, so all headlines stay visible. Keep new sidebar sections inside `Section` and inside the right tab.
- **Vertical resizing**: `components/Splitter` is the bar (pointer capture, arrows, double-click reset; owner measures in `onStart` and applies
  the offset in `onMove`). `Section` takes `resizeKey`: a user height is `flex: 0 1 <px>` (persisted as `sectionHeight.<key>`, clamped so the
  other boxes keep their minimum or headline; the last box has no bar). `Changes` splits the Unstaged/Staged lists with a persisted
  fraction (`changesSplit`) via `flex-grow`. The commit detail panel is not split.
- **Branch folders**: `sidebar/branchTree.tsx` (`buildRows`, `useClosedFolders`, `FolderRow`) turns names into folder/branch rows
  (folders first, nested by `/`, closed state not persisted, all open while a filter is active); local and remote lists both use it.
- **Sidebar filter**: `Sidebar` owns one text field (not persisted) and passes `filter` to `LocalBranches`, `Remotes`, `Stashes` and `Tags`,
  which list only matches via `matchesFilter` (`sidebar/filter.ts`: all words, case-insensitive). New sidebar lists should take it too.
- **Popovers/menus** use `ContextMenu` (portal to `<body>`; supports `checked`, `separatorBefore`,
  `danger`, `disabled`+`title`, and groups via `children`: a submenu snapped to the right of the parent item (flips left
  near the window edge), opened by hover with a 120 ms intent delay, click or ArrowRight; Escape/ArrowLeft close only
  the submenu; outside-click detection is `closest(".ctxmenu")` so it covers every panel). Split buttons (Fetch/Pull/Push) use `withMenu` in `SyncBar`: arrow click or
  right-click opens it; the arrow's `onMouseDown` stops propagation so the outside-click handler doesn't
  fight the toggle. Dialogs use `Modal`; simple yes/no uses `confirmDialog` (native, Tauri dialog plugin).
- **Right-click**: `App` suppresses the webview's native context menu everywhere except text fields (input/textarea,
  which keep cut/copy/paste). A component that wants a right-click action handles `onContextMenu` and calls
  `preventDefault()` itself; anything without a handler simply does nothing.
- **Destructive actions** confirm first, say what is lost, and prefer safe variants
  (`--force-with-lease`, safe checkout, abort on conflict). Disabled menu items carry a tooltip saying why.
- **Tests** build real repos in temp dirs (git2 and, for network behaviour, the system git with local bare
  remotes). Add a test with every backend feature. Commit messages end with the attribution trailer given in
  the session context.
- **Keyboard**: a global (window-level) `Escape` handler must ignore events from editable targets
  (`input`/`textarea`/`select`/contenteditable; xterm's hidden textarea counts) and must do nothing while a
  `.ctxmenu` or `.modal-backdrop` is open, because those handle their own Escape. `FileDiff` is the model.
  The list of shortcuts lives only in the table in `FEATURES.md`: keep it in sync. Implementation notes: Ctrl/Cmd+F (`RepoView`) works in text fields, is skipped inside
  `.xterm`, menus and dialogs, and its `preventDefault` keeps WebView2's own find bar away; Ctrl/Cmd+Z (`UndoButtons`) skips editable targets, menus, dialogs; ↑/↓ in the
  commit graph (`Graph`, `keyboard` prop: off while a diff, the rename panel or the rebase screen covers it) and through the right panel's files while a diff is open
  (`CommitDetail`, `Changes`) go through the `useArrowKeys` hook (plain arrows only; skips fields, menus and dialogs); the interactive rebase's keys (`InteractiveRebase`)
  are window-level and skip editable targets, menus, popups and `diffOpen`.
- **Texts** live in `src/i18n/en.ts`, never inline in components: labels, titles/tooltips, aria-labels, placeholders,
  confirmation texts, notices. Use `t.<section>.<key>` (strings), functions for texts with values (`t.sync.pushTo(upstream)`,
  pluralise with the local `plural` helper), and `fill(template, { name: <strong>…</strong> })` for sentences containing markup.
  Sections follow the features (`changes`, `graph`, `rebase`, ...); shared words go in `common`. Only English exists and no
  other language is planned: this is for tidiness, so don't add a locale switch. Backend (Rust) error messages are shown as
  they come and are not in the file. In `TerminalPanel` the import is `t as text`, because `t` is the xterm instance there.
- Match the surrounding comment density: short comments that explain *why*, none restating the code.

## Design notes (`docs/design/`)

The product decisions and implementation details of each feature live in `docs/design/`, one file per area. **Read the matching file before changing that area**, and
update it when a decision changes (new decisions go in the right file; split a file once it passes ~150 lines). What the user sees is in `docs/features/`.

| File | Read it for |
| --- | --- |
| `sync-and-remotes.md` | Pull (ff-only, diverged dialog), force push, credential prompts (`GIT_ASKPASS`), fetch failures, auto-fetch, multiple remotes and the target remote |
| `graph-and-branches.md` | Start screen, graph highlighting, checkout from the graph, remote branch checkout, branch rename, clean up merged, delete synced, the WIP row, commit search |
| `history-rewrites.md` | Interactive rebase, rename, drop, merge and its conflict view, rebase onto, fast-forward, reset |
| `worktrees.md` | Worktrees: where they go, branch rules, removal, the watcher, agents (terminal sessions owned by the backend, presets, status dots, merge back) |
| `undo.md` | The undo journal: snapshots, `Kind`s, how to make a command undoable |
| `changes-and-diffs.md` | Amend, working-tree diffs, stash, the Changes file menu, file history, hunks, stage/discard line(s), diff view modes and chrome |
| `files-tab.md` | The Files sidebar tab, the file viewer and editor, blame hint, terminal references |
| `settings.md` | The Settings dialog, `useSetting`, the terminal's branch and tag suggestions (shadow input line) |
| `building-and-release.md` | CI jobs, the build and run scripts and their gotchas, the app icon, release notes |

## Pitfalls learned the hard way

**libgit2 / git2 0.21**
- Many accessors return `Result`, not `Option`: `Commit::message()`, `summary()` (`Result<Option<&str>>`),
  `Signature::name()/email()`, `Reference::name()/shorthand()`. `StringArray::iter()` yields
  `Result<Option<&str>>` (flatten twice). `DiffOptions::max_size` takes `i64`. `mergehead_foreach` needs
  `&mut Repository`, so call it before creating a `Tree` that borrows the repo.
- **`Branch::rename` deletes the old ref before failing on a directory/file clash** (`feature` vs
  `feature/x`): the branch is lost. `rename_branch` pre-checks clashes and restores the branch on failure.
- `repo.commit(Some("refs/heads/x"), ...)` requires the first parent to be the ref's current tip (matters
  when building test histories).
- Use `disable_pathspec_match(true)` when a diff is limited by a literal file path.
- `diff_index_to_workdir` only emits lines for untracked files with `include_untracked(true)` **and**
  `show_untracked_content(true)`; `diff_tree_to_index` takes `None` for the tree on an unborn branch.

- Stashes: `stash_foreach`/`stash_save2` need `&mut Repository`, so code holding only `&Repository` uses
  `stash_index_of` (opens a second handle). A stash commit's parents are [base, saved index, untracked
  files]; the graph draws only the first and hides the other two rows, and `get_commit_detail`/`get_file_diff`
  read the untracked files from the third parent's tree. libgit2 creates that third commit even when there
  are no untracked files (empty tree), so don't treat its presence as meaningful. **libgit2's `stash_pop`
  returns success for a conflicting apply, leaves conflict markers and still drops the stash** (git keeps the
  stash): use `stash_apply`, check `index.has_conflicts()` (after `index.read(true)`), and only then
  `stash_drop`.

**git CLI**
- A rejected `--force-with-lease` push with `--atomic` prints "atomic push failed"; only fall back to a
  non-atomic push on "does not support --atomic", or a half-done rename results.
- Killing `git` leaves helpers (`git-remote-https`, ssh) holding the stdout/stderr pipes, so reading them
  blocks. `run_git_with` kills the whole process tree (`taskkill /T` on Windows, a process group on unix)
  and does not wait on the pipes after a timeout.
- Fetch rewrites `.git/FETCH_HEAD`, which the watcher sees; silent fetches pass `--no-write-fetch-head`
  (git >= 2.29, with a fallback for older git).

**Windows**
- ConPTY asks the terminal for the cursor position (`ESC[6n`) and waits for the answer. xterm.js replies
  automatically. A session without a panel has nobody to answer: the reader thread does (`terminal/mod.rs`), and strips the query from the output kept for replay.
- `canonicalize()` yields `\\?\` paths; strip the prefix before showing/storing them.
- Spawned git processes use `CREATE_NO_WINDOW`. Git config has `core.autocrlf=input`; CRLF warnings on
  commit are harmless.

**Styles**
- Styles were one hand-written `App.css`; a merge once dropped a closing `}` and every later rule silently
  stopped applying. They are SCSS now, so unbalanced braces fail `vite build` / `tauri dev` loudly.
- The migration to SCSS was verified by compiling the result and comparing it with the old stylesheet per
  selector (effective declarations with duplicates merged, plus a check that no two equal-specificity rules
  that can hit one element swapped order), and by injecting a defect to see the comparison catch it. Reuse that
  approach (postcss is in `node_modules`) for any large style refactor.
- A latent bug is kept on purpose: `.cd-meta p { margin: 0 }` (specificity 0,1,1) overrides `.cd-line` and
  `.cd-bodytext` `margin-top: 8px` (0,1,0), so those gaps never applied. Fixing it changes spacing visibly.

## Styles (SCSS)

Vite compiles SCSS (the `sass` package) in dev with hot reload and in `vite build` / `tauri build`; there is no
separate CSS step.

- **Global** (`src/styles/`, loaded first by `App.tsx` via `main.scss`): `_tokens` (SCSS constants: palette,
  tints, radii, shadows, fonts), `_mixins` (`section-label`, `truncate`, `text-field`, `floating`,
  `panel-footprint`), `_core` (forwards both), `_base` (reset, `:root` CSS variables, utilities), `_buttons`,
  `_switch`, `_filelist`, `_panel` (`.panel-head`), `_spinner` (`.spinner`: use it for every loading state; the graph shows one until its first page arrives, in `.graph` itself so the scroller and its `ResizeObserver` stay mounted). Only these emit CSS globally.
- **Per component**: a `.scss` next to the component, imported by it (`import "./Graph.scss"`), starting with
  `@use "@/styles/core" as *;` (the `@/` alias works in Sass through Vite). `Toolbar.scss` serves `SyncBar` and
  `BranchButton`; `Sidebar.scss` serves everything in `features/sidebar/`.
- **Theme colours stay CSS custom properties** (`--bg`, `--accent`, ...) so they can change at runtime; the SCSS
  tokens (`$bg`, `$accent`) just expand to `var(--bg)` etc. Use a token or mixin before inventing a new value.
- Scrollbars are styled globally in `_base.scss` (thin `::-webkit-scrollbar`, thumb = `--muted` at 35% via `color-mix`, plus
  `color-scheme: dark`; the standard `scrollbar-*` properties only as a fallback because Chromium ignores the webkit
  pseudo-elements once those are set). The xterm terminal draws its own scrollbar, coloured through its `theme`.
- Focusable list rows (`li[tabindex]`) have no focus outline (`_base.scss`): the selection highlight is the indicator.
- Class names are global (no CSS modules), so keep them specific to their feature. Within a file, keep the order
  hover, then selected, then "menu open" (`.ctx`): equal-specificity rules rely on source order.

## Working in this environment (Claude Code on the author's Windows machine)

- The Bash tool is Git Bash. A shell opened before a tool was installed may lack it: prefix
  `export PATH="$PATH:$HOME/.cargo/bin"` for cargo. If the user's own terminal says `cargo metadata ... program
  not found`, they need to restart VS Code so PATH refreshes.
- **Python is not installed.** Use Node for scripted edits.
- **Write scripts with the Write tool, not shell heredocs.** The shell layer mangled sequences such as a
  backslash followed by a backtick (a `sed` clean-up put a stray backtick at the start of every line), and
  complex quoting failed. Put scratch scripts in the session scratchpad, run them with `node "<windows path>"`,
  and verify results with `Read`/`grep` rather than assuming. Build escape characters from
  `String.fromCharCode` if a script must write them.
- **`node` resolves `/tmp` as `D:\tmp`**, unlike Git Bash. Pass real Windows paths (`cygpath -w`) to node.
- A reliable scripted-edit pattern: read the file, normalise `\r\n` to `\n`, `must(text.includes(anchor))`,
  replace, write back with the original EOL. `tsconfig.json`, `vite.config.ts`, `tauri.conf.json`, `index.html`
  and `main.rs` use CRLF; most other files are LF.
- **Splitting commits by feature when features share files**: stage the feature-only files with `git add`, and
  for shared files build the intended content (HEAD plus that feature's edit, or the working copy minus the
  other feature's block), then
  `git update-index --cacheinfo 100644,$(git hash-object -w --stdin),<path>`. Inspect with
  `git diff --cached --stat` before committing.
- The Vite dev server on port 1420 keeps running; a Rust change restarts the app, which returns to the
  welcome screen (open a repo again to see repo-view changes).

## Git workflow with this user

- Commit only when asked; **never push unless asked** (the user often pushes themselves). Branch is `main`,
  remote `origin` (`github.com/senorincognito/rusty-git`).
- The user sometimes commits between turns and switches branches (using this very app). Re-check
  `git status`/`git log` before committing; files can differ from what you last saw.
- End commit messages with the attribution trailer from the session context.
- They like concise "what changed / what's not done" summaries, honest notes about what was not verified in
  the running UI, and a question about commit grouping at the end. They sometimes write in German.

## Building and distribution

Details (CI jobs, build scripts and their gotchas, the icon, release notes) are in `docs/design/building-and-release.md`: read it before touching `.github/`, `scripts/` or a release.

- **Unsigned builds, no auto-updater** (decision): don't add signing or an updater unless asked; the options already discussed are in the design file.
- Build locally with `npm run build:run` (release build, then start it) or `scripts/build-release.*`; underneath it is `npm run tauri build`.
- CI lint and tests run on every push; `build` and `release` run **only on a `release/<version>` branch** (the name must equal the `package.json` version) and create a draft release.
- The version lives in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`; keep them in sync. The bundle identifier is `net.senorincognito.gitclient`, see "Persisted state".
- Release notes: `docs/releases/v<version>.md`, one per release, written when the version is bumped (copy the previous file's structure, list only what changed).
- The app shells out to the system `git`, so users need Git on PATH ("git executable not found" when missing). macOS builds can only be made on a Mac.
