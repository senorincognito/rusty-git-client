# CLAUDE.md

Guidance for working on **Rusty Git Client**: a desktop Git GUI in the spirit of GitKraken, built with
Tauri 2 (Rust backend, `git2`/libgit2 plus the system `git`) and React + TypeScript + Vite.
Targets Windows and macOS. `FEATURES.md` lists the user-facing features (update it with every feature) and `README.md` covers requirements, running
and building; this file is for contributors.

**Keep this file current.** The author has given standing permission to update it without asking: when work
adds a module or a rule, settles a product decision, or uncovers a quirk (library, git, Windows, tooling),
record it here in the right section, briefly and with the *why*. Edit existing entries rather than
duplicating them, and move finished items off "Not implemented yet".

## Commands

```sh
npm install
npm run tauri dev                                    # run the app (hot reload; a Rust change restarts it)
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
| `graph/mod.rs` | `features/graph` | `get_graph`: revwalk over all refs, lane layout computed in Rust, `on_head` flag per row |
| `graph/fast_forward.rs` | graph + branch context menus | `fast_forward_cmd`: move the checked-out branch forward to a commit id or full ref name |
| `graph/merge.rs` | branch context menus | `merge_branch_cmd`: merge a branch (full ref name) into the checked-out one via system git; returns `upToDate`/`fastForward`/`merged` |
| `graph/reset.rs` | graph context menu | `get_reset_info` (what a reset would remove/add, pushed count, uncommitted files), `reset_to_commit` (soft/mixed/hard) |
| `history/mod.rs` | graph context menu | shared by the rewrites below: `is_pushed`, `rewrite_plan`, `rebuild_with_messages`, `move_head_to`, `blocking` |
| `history/rename.rs` | `features/rename` | `get_rename_info`, `rename_commit_message` (rewrites the commit and its descendants) |
| `history/rebase.rs` | `features/rebase` | `get_rebase_plan`, `apply_rebase_cmd` (interactive rebase: pick/reword/squash/drop/reorder), `replay_group` |
| `history/drop.rs` | graph context menu | `get_drop_info`, `drop_latest_commit` (drop one commit) |
| `history/test_support.rs` | tests only | small repositories with real commits for the history tests |
| `changes/mod.rs` | `features/changes` | status, stage/unstage, `discard_paths` (whole-file discard), `create_commit` (new or `amend`), `get_head_commit` |
| `changes/hunks.rs` | diff view | `stage_hunk`, `discard_hunk`, `unstage_hunk`: apply one hunk of a file's staged/unstaged changes (see Product decisions) |
| `commit/file_history.rs` | `features/commit/FileHistory` | `get_file_history`: `git log --follow --name-status` for one file (≤ 500 commits), parsed into id/author/time/status/path-at-that-commit/oldPath |
| `commit/mod.rs` | `features/commit` | all diff rendering: `get_commit_detail` (files of a commit, renames), `get_file_diff` (a commit's file), `get_working_diff` (staged/unstaged file); shared `diff_options` + `render_diff` |
| `sidebar/branches.rs` | `features/sidebar` | list, create+checkout, checkout, delete, rename (local) |
| `sidebar/cleanup.rs` | `features/sidebar` | "Clean up merged": `get_merged_branches(path, remote)`, `delete_merged_local` (one undo step), `delete_merged_remote` (`git push <remote> --delete`, not undoable) |
| `sidebar/remotes.rs` | `features/sidebar` | `get_remotes` (every remote with branches, `isTarget`, tracking count), `add_remote_cmd`, `set_remote_url_cmd`, `delete_remote_cmd`, `set_target_remote`; delete/rename remote branches take a `remote` argument |
| `sidebar/stash.rs` | `features/sidebar`, `features/changes` | `get_stashes`, `create_stash` (stashes everything incl. untracked), `stash_paths_cmd` (selected files, via system git), `pop_stash_cmd`, `apply_stash_cmd`, `drop_stash_cmd`; helpers `stash_index_of`, `untracked_tree` |
| `toolbar/sync.rs` | `features/toolbar` | fetch / pull / push / force push / auto-fetch / diverged pull; `run_git`, `run_git_with` |
| `auth/mod.rs` | `features/auth` | credential prompts: `GIT_ASKPASS`/`SSH_ASKPASS` script, loopback socket, `credentials-request` event, `answer_credentials` |
| `undo/mod.rs` | `features/toolbar/UndoButtons` | the undo journal: `recorded(path, label, kind, || op)` wraps a command, `get_undo_state`, `undo_cmd`, `redo_cmd` |
| `terminal/mod.rs` | `features/terminal` | PTY sessions (`portable-pty`) feeding the xterm.js panel |

Modules reach each other by full path (`crate::toolbar::sync::run_git`, `crate::changes::status_of`,
`crate::sidebar::stash::save_stash`). Tests live in a `#[cfg(test)] mod tests` at the bottom of the file they test.

### Frontend (`src/`, `@/` = `src/`)

```
App.tsx                    Welcome or RepoView
api/                       one typed wrapper file per backend module
features/welcome|repo|graph|changes|commit|rename|rebase|sidebar|toolbar|terminal/
components/                generic UI: ContextMenu, Modal, ResizablePanel, Section, FileBadge
hooks/                     useLatestRequest, usePersistentState, useAutoFetch
i18n/                      en.ts: every user-facing text; index.ts exports it as `t`, plus `fill`
styles/                    global SCSS: tokens, mixins, base, buttons, switch, filelist, panel (see Styles)
*.scss next to components  each component's own styles, imported by that component
```

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
`autoFetch.enabled` (default true), `autoFetch.seconds` (default 180), `diff.fullFile` (default true), `sectionHeight.<local|remotes|stashes>` (px), `changesSplit` (fraction).
All of it is keyed by the bundle identifier `com.gitclient.app`, which is **deliberately unchanged** by the
app rename so users keep their data. Don't change it casually.

## Conventions

- **Commands** are `async` and run blocking work in `tauri::async_runtime::spawn_blocking` (see the
  `blocking` helpers). Register each in `lib.rs`. Serde structs use `rename_all = "camelCase"`; Tauri maps
  JS `oldPath` to Rust `old_path` automatically.
- **git2 vs system git.** Local reads/writes use `git2`. Anything that talks to a remote or needs
  credentials, merges or rebases uses the system `git` via `run_git` / `run_git_with` (user's credential
  helper, SSH agent, config apply; `GIT_TERMINAL_PROMPT=0`, no console window on Windows).
- **Async UI requests** must ignore out-of-order responses: use `useLatestRequest`
  (`const isCurrent = start(); ...; if (isCurrent()) set(...)`).
- **Clean up merged** (`sidebar/cleanup.rs`; item in the "⋯" menus of Local branches and Remotes): merged = the tip is a *strict* ancestor of a main tip (`graph_descendant_of(base, tip)`), so branches sitting exactly on main are excluded on purpose (new branches). Main = local `main`/`master`; per remote `<remote>/HEAD`'s target, else `main`/`master`. Local candidates are checked against the local main *and* every remote main; remote candidates only against their own remote's main (that is what the server has). Never offered: the checked-out branch, `PROTECTED` names (main, master, develop, dev, trunk) and the base's own name, and a remote branch that is the checked-out branch's upstream. The UI asks the backend for the list, shows it in a confirmation, and the delete commands recompute the list and only delete the intersection. Remote deletion runs one `push --delete` per remote with the system git and is outside the undo journal; the confirmation says so.
- **Delete synced branches** (Local branches "⋯" menu, `delete_synced_branches`): "synced" = has an upstream and the local tip equals the upstream tip (`ahead == 0 && behind == 0` in `BranchInfo`, so relative to the last fetch), never the checked-out branch. The UI lists them in the confirmation; the backend re-checks each name before deleting and silently skips any that no longer qualify. One undo step (`Kind::Keep`).
- **Sidebar section headlines** can carry a "⋯" menu (`Section`'s `action` prop): Local branches (*New branch…*, which shows
  the same `NewBranchForm` the toolbar's Branch button uses, inline) and Remotes. Keep branch creation in that one form.
- **Sidebar layout**: `.sidebar` does not scroll itself; each open `Section` is a shrinkable flex item (min height 96px) whose
  `.sidebox-body` scrolls, so all headlines stay visible. Keep new sidebar sections inside `Section`.
- **Vertical resizing**: `components/Splitter` is the bar (pointer capture, arrows, double-click reset; owner measures in `onStart` and applies
  the offset in `onMove`). `Section` takes `resizeKey`: a user height is `flex: 0 1 <px>` (persisted as `sectionHeight.<key>`, clamped so the
  other boxes keep their minimum or headline; the last box has no bar). `Changes` splits the Unstaged/Staged lists with a persisted
  fraction (`changesSplit`) via `flex-grow`. The commit detail panel is not split.
- **Branch folders**: `sidebar/branchTree.tsx` (`buildRows`, `useClosedFolders`, `FolderRow`) turns names into folder/branch rows
  (folders first, nested by `/`, closed state not persisted, all open while a filter is active); local and remote lists both use it.
- **Sidebar filter**: `Sidebar` owns one text field (not persisted) and passes `filter` to `LocalBranches`, `Remotes` and `Stashes`,
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
  Existing shortcuts: Esc (close diff / menus / dialogs / editors), Ctrl+\` terminal (`RepoView`),
  Ctrl/Cmd+Enter commit and rename-update, Ctrl/Cmd+Z / +Shift+Z (Ctrl+Y) undo / redo (`UndoButtons`, skips editable targets, menus, dialogs), arrow keys on resize handles, ↑/↓ in the commit graph (`Graph`, `keyboard` prop: off while a diff, the rename panel or the
  rebase screen covers it) and through the right panel's files while a diff is open (`CommitDetail`, `Changes`), all via
  the `useArrowKeys` hook (plain arrows only; skips fields, menus and dialogs), ↑/↓ (and Ctrl/Cmd+↑/↓ to move) in the
  interactive rebase (`InteractiveRebase`, window-level; skips editable targets, menus, popups and `diffOpen`). Keep the shortcuts table in `FEATURES.md` in sync.
- **Texts** live in `src/i18n/en.ts`, never inline in components: labels, titles/tooltips, aria-labels, placeholders,
  confirmation texts, notices. Use `t.<section>.<key>` (strings), functions for texts with values (`t.sync.pushTo(upstream)`,
  pluralise with the local `plural` helper), and `fill(template, { name: <strong>…</strong> })` for sentences containing markup.
  Sections follow the features (`changes`, `graph`, `rebase`, ...); shared words go in `common`. Only English exists and no
  other language is planned: this is for tidiness, so don't add a locale switch. Backend (Rust) error messages are shown as
  they come and are not in the file. In `TerminalPanel` the import is `t as text`, because `t` is the xterm instance there.
- Match the surrounding comment density: short comments that explain *why*, none restating the code.

## Product decisions already made (keep consistent)

- **Start screen** (`features/welcome/Welcome`): two panels (CSS grid; stacked under 800px): Recent on the left; on the right the repositories found in one remembered folder (`repo_folder.json`), filtered with `matchesFilter` from the sidebar, opened with the same `openRepo` (so they also become recent). One folder only; several are a possible extension. Not live: a rescan (↻) or reopening the screen refreshes it. Cards show the branch and last-commit age but no change status (opening each repository to count changes would be slow for hundreds).
- **Pull** fast-forwards only (`--ff-only`). If the branches diverged, a dialog lists both sides and offers
  **Merge (default, preselected)**, Rebase, Cancel. The Pull button's arrow/right-click menu has
  "Pull (merge)" and "Pull (rebase)" to skip the dialog. All run with `--autostash`. On **conflicts the
  operation is aborted automatically** (conflicting files named, repo left untouched) since there is no
  conflict UI yet. `pull.rebase` is intentionally not consulted.
- **Force push** is `git push --force-with-lease`, behind a confirmation that counts the remote commits
  that will be discarded. Only offered when the branch has an upstream. **Push on a diverged branch** (known ahead *and* behind from the last
  status, no network check) opens `PushDialog` (same two commit lists as the pull dialog, `getDivergence`) instead of pushing: *Force push*
  (runs the same lease push; the dialog is its confirmation) or *Cancel* (default focus). A push rejected because the remote moved since the
  last fetch still shows git's own message, and a force push then fails on the lease as intended.
- **Undo / Redo** (`undo/mod.rs`, `UndoButtons` in the title bar + Ctrl/Cmd+Z): every command that changes the repository is wrapped in `undo::recorded(&path, label, Kind, || ...)`
  (the closure starts the real work *after* the "before" snapshot; the entry is pushed only if the op succeeded and the state differs). A `Snapshot` = HEAD (branch name or
  detached id), all local branch tips, the stash list (id + message), and for some kinds the index and the whole working directory as **trees** (built in memory with
  `update_all` + `add_all`, so untracked files are included, ignored ones not; objects stay in the odb). `Kind` decides what is captured and restored: `Keep` (refs only: commit,
  amend, soft reset, branch create/rename/delete, stash drop; undoing a commit leaves the content staged because the index is left alone), `Switch` (refs + safe checkout of the old
  tree, never overwriting newer edits: merge, ff, pull, rebase family, rename, drop, checkout), `Index` (+ index tree: stage/unstage, hunks, mixed reset), `Full` (+ index and workdir trees,
  restored by `restore_workdir` which rewrites changed files and deletes added ones: discard, stash create/pop/apply, hard reset). Undo/Redo first compare the *current* snapshot with the
  entry's expected one; any difference (the terminal did something) = nothing happens, the journal for that repo is cleared. Stashes are restored by dropping them all and
  `git stash store -m <message> <id>` in order (the stash commits outlive a drop). The journal is in memory per repo (`JOURNALS`, keyed by the git dir), max 50 steps; a new action clears
  Redo. Not recorded: fetch, push, force push, remote edits (they can't be taken back locally). To make a new command undoable wrap it and choose the `Kind`; add a label that reads like
  "Undo: <label>". Untested in the running UI; the journal logic itself is unit-tested with real repositories.
- **Credential prompts** (`auth/mod.rs`, `features/auth/CredentialPrompt`, mounted in `App`): `run_git` (manual operations) sets
  `GIT_ASKPASS`/`SSH_ASKPASS` (+`SSH_ASKPASS_REQUIRE=force`) to a temp-dir script (not the app data dir: git splits the command at the
  space in `Application Support`) that runs this exe as `--askpass <prompt>` (handled first thing in `run()`); it asks the app over a
  token-checked loopback socket, the UI answers via `answer_credentials`. `run_git_with` (auto-fetch, timeouts) never prompts, so
  background work still fails fast with `GIT_TERMINAL_PROMPT=0`. Unix only (Windows has Git Credential Manager). The round trip is
  unit-tested; the unix script and a real macOS push are untested here. Credentials are not stored by the app: git's credential
  helper does that if one is configured.
- **Fetch failures** show as a ⚠ beside "origin" in the sidebar (tooltip = git's message). `SyncBar` combines its last manual
  fetch error with `useAutoFetch().error` and reports it up via `onFetchError` to `RepoView`, which passes `fetchError` to
  `Sidebar` > `Remotes`. Any successful fetch, pull or background fetch clears it; a failed push does not set it.
- **Auto-fetch**: on by default, every 180 s, only while the window is focused/visible, never overlapping a
  manual git operation, exponential backoff when offline, pauses (does not retry) on auth errors, resumes
  after a successful manual fetch/pull/push. Uses `--no-write-fetch-head --no-auto-gc`, 90 s timeout.
- **Amend** switch pre-fills the last message, keeps the author, makes you the committer, works with
  nothing staged, warns when the commit is already pushed.
- **Interactive rebase** (graph context menu, `features/rebase/InteractiveRebase`): a full screen that covers the sidebar
  and the centre (`RepoView` wraps both in `.workarea`; they stay mounted, `visibility: hidden` + `inert`, so the graph
  keeps its scroll position); the right panel stays: `Changes`, or `CommitDetail` of the commit clicked in the list (`selectedId`/`onSelectCommit`, the shared
  `selectedCommit` state). A file clicked there opens `FileDiff` in `.rebase-diff` over the rebase screen (the normal centre
  `FileDiff` is suppressed while rebasing); `diffOpen` stops the rebase screen's own Escape. Working diffs are ignored meanwhile.
  The clicked commit is the base and is not edited; `rebase_plan` lists the commits after it on HEAD's first-parent line
  (max 500, newest first) plus `headId`. Each row has an action `<select>`: **pick**, **reword** (a `Modal` popup;
  a commit only becomes "reword" when the popup is confirmed), **squash** and **drop**. `apply_rebase` (command `apply_rebase_cmd`,
  steps `{id, action, message?}`, unlisted commits are picks) groups each squashed commit with the next older non-squashed
  commit (a chain of squashes all go into the one that starts it), then rebuilds from the oldest changed group: parents of
  the group's oldest commit, tree of its newest commit, author of the oldest, you as committer, message = the older
  message + blank line + each squashed message. No cherry-picks (each commit already contains its predecessors), so no
  conflicts and no clean working directory are needed, and the tip's tree never changes. The oldest commit and merge
  commits can't be squashed (`RebaseGroup`), nor squashed into a dropped commit; it refuses when HEAD moved since the plan.
  **Drop** changes content, so from the first dropped group on `replay_group` re-creates each kept commit with
  `cherrypick_commit` onto the rebuilt parent (a dropped commit maps to its rebuilt parent in `rebuilt`); a conflict
  abandons everything with the commit and files named, a merge commit after the first drop is refused, the working
  directory must be clean and a hard reset (not `move_head_to`) finishes. Groups before the first drop still reuse trees.
  **Reorder** (context menu *Move commit up/down*, UI state `order`) is sent as `order` (ids, newest first) to
  `apply_rebase_ordered`; groups and squashes are built on the new order, `first_moved` is the first oldest-first position
  that differs from the plan, and from there `replay_group` re-creates the commits one by one on `tip` (the previous
  rebuilt commit; parents are `[tip]` in replay mode, mapped original parents otherwise). Same clean-working-directory,
  merge and conflict rules as Drop. The UI computes `problem` (an action that no longer fits after a move) to block Start.
- **Rename commit** (graph context menu) only for commits on the current branch. Rebuilds the commit and
  every later commit with identical trees/authors/dates, then moves the branch; other branches keep the old
  history. Warns about rewritten descendants and pushed commits.
- **Drop commit** (graph context menu, `history/drop.rs` `drop_plan`/`drop_commit`) works for any commit on the checked-out
  branch's own first-parent line when no merge commit lies between it and the tip, and the commit has a parent.
  The commits after it are re-created in memory with `cherrypick_commit` (3-way merge, original author, committer
  and message; empty results are kept, not silently dropped); a conflict aborts with the commit and files named
  and nothing changed. Only then does a hard reset move the branch, index and working directory, so the working
  directory must be clean. The confirmation (RepoView `dropCommit`, data from `get_drop_info`: later commits,
  pushed counts, merge flag) warns about rewritten ids, pushed history (force push), merge commits, other refs
  keeping the old history and lost signatures. Errors use the native `showError` dialog. **Revert commit** (the
  non-rewriting alternative for pushed commits) is not built.
- **Merge** (`graph/merge.rs`; *Merge <branch> into <current>* in a local branch's menu, *Merge <remote>/<branch> into the current branch* in a remote branch's, via `RepoView.mergeInto`): merges into the checked-out branch only, after a confirmation. Runs `git merge --no-edit --autostash <full ref>` (system git, so the user's identity/config apply; the ref must start with `refs/`, which also keeps it from being read as an option). Same conflict policy as Pull: no conflict UI, so a conflicted merge is aborted with `cancel_unfinished` (files named, nothing changed). Fast-forwardable branches fast-forward (no merge commit); an already contained branch is reported with an info box. Refused on a detached HEAD or while another operation is in progress.
- **Fast-forward** (`graph/fast_forward.rs`; *Fast-forward to this commit* in the graph menu, *Fast-forward <current> to <branch>* in a local branch's menu, both through `RepoView.fastForwardTo`): `git merge --ff-only` semantics on the checked-out branch only. The target must be a strict
  descendant of HEAD (otherwise errors: already contained, or diverged with both counts), HEAD must be a branch, the repo state clean. The files are
  checked out safely first (uncommitted changes in the way abort it, nothing moves), then the branch ref moves. No confirmation (nothing is lost);
  errors use `showError`. The UI only pre-disables the obvious cases (stash, already on the branch's line, the branch itself); the rest is the backend's message.
- **Reset** (graph context menu, `graph/reset.rs`): a *Reset to this commit* group whose submenu offers Soft / Mixed / Hard on any non-stash commit,
  each behind a confirmation whose text is built by `features/repo/describeReset.ts` from `get_reset_info` (commits
  removed with the newest few listed, commits gained, pushed count, uncommitted file count, ancestor or jump to another
  line). It is plain `git2` `reset` (Soft/Mixed/Hard), also allowed on a detached HEAD, refused during a merge/rebase
  and on an empty repo. Hard leaves untracked files alone; only uncommitted changes are unrecoverable (removed commits
  stay in the reflog).
- **Multiple remotes**: the sidebar lists every remote (`Remotes.tsx`, one block each); adding one and choosing the target
  live in the "⋯" menu of the section headline (`Section`'s `action` prop; its mouse-down stops propagation like the split buttons). The **target remote**
  (`remotes::target_remote`) is where Push publishes a branch without an upstream (`push --set-upstream`): the one the user
  chose (repo-local git config `rustygit.targetremote`), else `origin`, else the first remote; deleting the target clears the
  choice. Everything else already follows each branch's own upstream (fetch is `--all`, pull/push use `branch.<n>.remote`).
  Removing a remote (`remote_delete`) drops its remote-tracking refs and the upstream of branches that tracked it, never
  touches the server, and confirms first. Names are checked with `Remote::is_valid_name`. The fetch-failure ⚠ shows on every
  remote because the fetch covers all of them.
- **Graph highlighting** (`Graph.tsx` `isCurrent`, `Graph.scss`): the row of the commit HEAD points at (a ref label with `isHead`, also a detached HEAD) gets `.current` (accent bar via inset
  box-shadow, faint tint placed *before* hover/selected so they win, bold subject) and a ring around its node; rows with `!onHead` (not reachable from HEAD, not WIP/stash) get `.off` (text at 60% opacity). The accent colour is
  hard-coded as `rgba(63, 167, 160, …)` for the tints because the SCSS token is a CSS variable.
- **Checkout from the graph** (`Graph`: `checkoutTargets`, `checkoutItems`, `doubleClickTarget`; `RepoView.checkoutRef`): a row whose `refs` include a `branch` or `remote` label that is not `isHead` (so never the checked-out branch
  or a detached HEAD) gets *Check out <name>* first in its menu (a submenu if several) and a double-click does it (local branch preferred, else the single remote one). Local = `checkout_local_branch`;
  remote = `checkout_remote_branch` with the label split at its first `/` (remote names with a slash would fail with "not found"). Stashes and the WIP row offer nothing.
- **Remote branch checkout** (`checkout_remote_branch`, double-click or first item of its menu): creates local `<name>` at the remote tip with
  upstream `<remote>/<name>` and checks it out safely (the new branch is removed again if the checkout fails); an existing local
  branch is reused only if it already tracks that remote branch, otherwise it errors rather than taking it over.
- **Branch rename** (inline editor in the sidebar) is not allowed for the checked-out branch (local) or the
  branch the checked-out branch tracks (remote). Remote rename = one atomic push of the new name plus
  deletion of the old one, guarded by a lease; local branches that tracked it are repointed.
- **Working-tree diffs** (click a file in Changes): *unstaged* = index vs file on disk (untracked files show as
  all additions), *staged* = HEAD vs index (on an unborn branch everything staged is an addition). Staging
  buttons on a row stop propagation so they don't also open the diff. Committing closes an open working diff.
- **Uncommitted changes** are a pseudo commit (`WIP_ID` "WIP", `isWip`) inserted by `build_graph` as the first
  row when HEAD has a commit and `status_of` reports changes: a dashed lane (`Edge.dashed`) leads down to HEAD's
  commit, so it lands in that commit's lane. Clicking it calls `closeCommit()` (right panel returns to `Changes`);
  it counts as selected whenever no commit is selected. It has no context menu. File edits aren't watched, so
  `Graph` also refetches on window focus.
- **Stash** moves all uncommitted changes, **including untracked files** (not ignored ones), into a stash with an
  optional message (`Stash…` button beside Commit and a *Stash* button in the title bar between `SyncBar` and the terminal toggle, both opening `StashDialog`; the title bar one is
  owned by `RepoView`, which counts the changes with `useWorkingChangeCount`). Stashes are visible in the graph (hollow node
  off the base commit, `stash@{n}` chip, row `isStash`) and in a left-panel `Stashes` section; clicking either
  opens the stash's `CommitDetail`. **Pop** is a button in that panel's bottom footer (`cd-footer`, mirroring
  the Stash button under the staging lists) and a *Pop stash* item in the right-click menu of a Stashes row
  (`pop_stash`: apply, check for conflicts, then drop;
  re-stages what was staged). Pop requires a clean working directory (disabled in the UI via
  `useWorkingChangeCount`, enforced in Rust) so a
  conflicting pop can be undone exactly (`restore_clean`) with the stash kept. **Delete stash** (`drop_stash_cmd`, Stashes right-click,
  confirmation first) just calls `stash_drop` by commit id and needs no clean working directory. **Apply** (`apply_stash_cmd`, `apply_stash(repo, id, remove)`; pop is `apply_stash(.., true)`) is the same
  clean-directory apply and conflict undo without the final drop. Both menus offer it: Stashes list and, for `isStash` rows, the graph's context menu (which
  then shows only Apply / Pop / Delete stash, via `RepoView.stashAction`; `hasChanges` disables Apply and Pop).
- **File context menu** (right-click a row in `Changes`): Unstaged = Stage / Discard / Stash, Staged = Unstage / Stash. Discard
  (`discard_paths`) restores from the index (so staged edits survive) or deletes an untracked file, only for paths that
  really have unstaged changes, never for conflicted files, always after a confirmation. Single-file **Stash**
  runs `git --literal-pathspecs stash push --include-untracked -- <paths>`: libgit2's path-limited `stash_save_ext` also
  cleans the files that were NOT selected, and git2 cannot set its message, so don't use it.
- **File history** (*File history* in the Changes file menu, not for new files, and in the file menu of `CommitDetail`, which then passes `from` = the selected commit so
  the log starts there: `get_file_history(path, file, from)` takes only a hex id, validated with `Oid::from_str` before it reaches git; no menu for stash files or during a rebase):
  `RepoView` keeps `history: { file, from, entry }`. `FileHistory` (centre, `center-pane`,
  stays mounted but `hidden` while a commit's diff is open so the scroll position survives; its Escape is off then via `active`) lists
  `get_file_history`; a click sets `entry` and shows the existing `FileDiff` with `source` = that commit and the path *as it was in that commit*
  (+ `oldPath` for renames), with `backLabel`/`backHint` overriding the back button. Uses system git (`--follow` is not available in libgit2) with
  `--literal-pathspecs`; merge commits without a change to the file simply don't appear. Selecting a commit/working file or starting a rebase closes it.
- **Hunks**: a hunk is a maximal run of added/removed lines (`DiffLine::block`, numbered in file order and identical in
  full-file and context-only views); `FileDiff::blocks` holds one FNV fingerprint per hunk. Every diff view shows a
  heading row per hunk; only the *unstaged* view of a tracked, non-conflicted, non-binary file gets **Stage hunk** /
  **Discard hunk**, and the *staged* view gets **Unstage hunk** (`stage_hunk_cmd` / `discard_hunk_cmd` /
  `unstage_hunk_cmd`, which re-diff and refuse with a "changed since this diff
  was shown" error when the fingerprint no longer matches). They do not build patches: `changes/hunks.rs` rebuilds the
  index blob (stage) or the file on disk (discard) line by line from the full-file diff using each line's raw bytes,
  so CRLF files and a missing final newline survive (discard re-inserts restored lines with the file's own EOL).
  Staging a deletion removes the index entry; discarding one checks the file out of the index. Unstaging rebuilds
  the index blob from the HEAD-to-index diff with that hunk reverted to HEAD's lines; if the file is not in HEAD
  (new file, or a branch without commits) and nothing is left, the index entry is removed, and a staged deletion
  is re-added from HEAD. The file on disk is never touched. Discarding a staged hunk is not offered.
- In the full-file view `FileDiff` draws change markers over the vertical scrollbar (`.fd-marks`: runs of add/del rows as percentages of the
  virtual list, `pointer-events: none`, narrower than the 10px scrollbar so the thumb stays visible; shown only when the list scrolls).
- `FileDiff` header ▲ / ▼ buttons jump between hunk heading rows (smooth scroll, two rows of context above; "where we are" = scrollTop + 2 rows, so
  repeated clicks always advance; disabled at the first/last hunk and when the list can't scroll further). No keyboard shortcut: ↑/↓ already move
  between files while a diff is open.
- Commit detail diffs are against the **first parent**; renames detected; 2000-file and 20 000-line caps;
  binary/over-5 MB files are not previewed. The diff view has a "Full file" switch (default on).

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
  automatically; terminal tests must reply `ESC[1;1R` themselves.
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
  `_switch`, `_filelist`, `_panel` (`.panel-head`). Only these emit CSS globally.
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

**Decision: unsigned builds, no auto-updater.** The author builds locally with the scripts below, or takes the installers
from the CI workflow (next paragraph). Don't add signing or an updater unless asked; if distribution to others comes up, the options already discussed are:
`tauri-action` on GitHub Actions (builds Windows + macOS and attaches them to a Release), `tauri-plugin-updater`
(needs its own signing key pair, kept as a CI secret and never committed, plus a `latest.json` on the
Release), and a Windows code-signing certificate to avoid the SmartScreen "unknown publisher" warning.

- **CI** (`.github/workflows/ci.yml`): jobs `frontend` (ESLint, `npm run build`), `rust-lint` (rustfmt, clippy `-D warnings`),
  `rust-test` (Windows and macOS only, `fail-fast: false`) and, after all three, `build` (Windows NSIS+MSI; macOS universal
  DMG via `--target universal-apple-darwin`) uploading artifacts. Lint and tests run on every push and PR; `build` and
  `release` run **only on a `release/<version>` branch** (the name must equal the `package.json` version, checked in `build`),
  and `release` creates a **draft** release `v<version>` with `gh release create` targeting that commit. No third-party release action, no
  secrets beyond `GITHUB_TOKEN`. Builds are unsigned (macOS needs right-click > Open). Nothing here has run on GitHub yet when this
  was written: if a job fails, fix the workflow from the log (Linux needs the webkit2gtk/xdo/appindicator packages just to compile).
  ESLint (`eslint.config.js`) enables only `rules-of-hooks` and `exhaustive-deps` from the react-hooks plugin: its React Compiler
  rules flag deliberate patterns (state reset in effects on `path` change, ref sync during render).
  **The tests are not run on Linux**: that job hung (after the `terminal` chunker test; the PTY test forking a shell while
  other tests spawn git on other threads is the suspect, unconfirmed), so the Ubuntu leg was dropped. Re-enable it by adding `ubuntu-latest` to the matrix (it needs the webkit2gtk/xdo/appindicator packages). Every
  job has a timeout. Lint (`rust-lint`) and the frontend still run on Ubuntu.
- Build with the scripts in `scripts/`: `build-release.cmd` (Windows launcher, double-clickable; runs the
  `.ps1` with `-ExecutionPolicy Bypass` because the default policy blocks unsigned .ps1 files) and
  `build-release.sh` (macOS/Linux/Git Bash; tested here only under Git Bash on Windows). Both check the
  toolchain, run `npm ci` if needed, build and list the fresh artifacts; options: `-Bundles all|nsis|msi|none`
  / `--bundles`, `--no-bundle`, skip install, open folder. `.gitattributes` pins their line endings (sh = LF,
  cmd/ps1 = CRLF), and the `.sh` must be committed with the executable bit
  (`git update-index --chmod=+x scripts/build-release.sh`). Underneath it is `npm run tauri build` (verified
  working, about 2.5 minutes for a cold release build, about 1.5 minutes for a rebuild). It produces, under `src-tauri/target/release/`: `rusty-git-client.exe` (6.9 MB, runs
  standalone given WebView2), `bundle/nsis/Rusty Git Client_0.1.0_x64-setup.exe` (2.2 MB) and
  `bundle/msi/Rusty Git Client_0.1.0_x64_en-US.msi` (3.1 MB). The first build downloads the WiX and NSIS tools
  from GitHub. All of `target/` and `dist/` is git-ignored. Run it with `export PATH="$PATH:$HOME/.cargo/bin"`
  in the Bash tool; a cold build prints one line per crate, so redirect output to a log in the scratchpad and
  run it in the background.
- Gotchas from writing the scripts: the `.cmd` pauses at the end only when started with no arguments (the
  double-click case); set `NOPAUSE=1` when another program runs it. In the `.sh`, `set -e` + `pipefail`
  turns a failing `find` on a missing folder (`bundle/macos` on Windows) into a silent script failure: guard
  such commands with `|| true`. Installers are listed only if newer than the run's start, so stale ones from an
  earlier build aren't reported as results. Keep the `.ps1` ASCII-only (Windows PowerShell 5.1 misreads
  BOM-less UTF-8).
- **Release notes** live in `docs/releases/v<version>.md` (the name matches the tag `release/<version>` creates), one file per
  release, written for the people who download it: requirements, downloads, what is in it, known limitations. Write the next
  one when the version is bumped; copy the structure of the previous file and list only what changed.
- macOS builds can only be made on a Mac.
- **The app shells out to the system `git`**, so users need Git installed and on PATH; the installers cannot
  bundle it. The app reports "git executable not found" when it is missing.
- Still the Tauri placeholder icons in `src-tauri/icons/` (replace via `npx tauri icon <logo.png>`).
- The version lives in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`; keep them in sync.
- The bundle identifier `com.gitclient.app` stays (it keys the saved data), see "Persisted state". `tauri build`
  warns that an identifier ending in `.app` is "not recommended" because it clashes with the macOS bundle
  extension. Harmless on Windows; if macOS builds are ever made, consider switching the identifier then (which
  resets saved settings and the recent-repos list once).

## Not implemented yet

Tags in the sidebar; line-level (single line) staging and unstaging; merge/rebase as standalone actions; discard
changes and stash; conflict resolution UI (pulls with conflicts are aborted); syntax highlighting and intra-line diff highlighting; side-by-side diff; a
"you rewrote pushed history, force push instead" hint in the diverged-pull dialog; a conflict preview
(`git merge-tree`) before pulling.
