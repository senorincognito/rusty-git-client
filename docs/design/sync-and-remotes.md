# Sync and remotes: design notes

Contributor notes moved out of CLAUDE.md: how it is built and why. What the user sees is described in `docs/features/`. Covers pull, push, credential prompts, fetch and auto-fetch, multiple remotes.

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

- **Multiple remotes**: the sidebar lists every remote (`Remotes.tsx`, one block each); adding one and choosing the target
  live in the "⋯" menu of the section headline (`Section`'s `action` prop; its mouse-down stops propagation like the split buttons). The **target remote**
  (`remotes::target_remote`) is where Push publishes a branch without an upstream (`push --set-upstream`): the one the user
  chose (repo-local git config `rustygit.targetremote`), else `origin`, else the first remote; deleting the target clears the
  choice. Everything else already follows each branch's own upstream (fetch is `--all`, pull/push use `branch.<n>.remote`).
  Removing a remote (`remote_delete`) drops its remote-tracking refs and the upstream of branches that tracked it, never
  touches the server, and confirms first. Names are checked with `Remote::is_valid_name`. The fetch-failure ⚠ shows on every
  remote because the fetch covers all of them.
