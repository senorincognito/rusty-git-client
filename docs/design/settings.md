# Settings, terminal suggestions and the command list

## Settings
- `features/settings/SettingsDialog` (a `Modal`) is opened from `App`: the ⚙ in `TabBar` (pinned right with
  `position: sticky`) and `Welcome`'s header, and `Ctrl`/`Cmd` + `,` (a window handler in `App`; the terminal's key
  handler returns `false` for that chord so xterm doesn't swallow it). It lives in `App`, not `RepoView`, so it also
  works on the start screen and survives a tab switch.
- Values are read with `hooks/useSetting` (`useSyncExternalStore` over a module-level cache + `localStorage`), unlike
  `usePersistentState`, whose copy is private to its component: a setting changed in the dialog has to reach a
  `TerminalPanel` that is already mounted. Add a setting as a hook next to `useTerminalAutocomplete` and a row in the
  dialog. Existing keys (`autoFetch.*`, `diff.mode`) stay where they are, with their own UI.

- **Agent presets** (`useAgentPresets`, key `agents.presets`) are a list edited row by row in the dialog; every keystroke saves. Incomplete rows are kept while typing and filtered out where the presets are used (`usablePresets`). See `worktrees.md` for what they do.

## Terminal suggestions
- The shell owns the input line, so `completion.ts` (`LineTracker`) keeps a *shadow* copy from `onData`: plain
  characters, Backspace (`\x7f`), Ctrl+U and Ctrl+C/Enter are understood; anything else (escape sequences, Tab,
  Ctrl+R, a chunk containing `\r`) marks it invalid until the next Enter/Ctrl+C. Wrong suggestions are worse than
  none, so it fails closed. The alternate screen buffer (less, vim) also turns it off.
- `completionContext` (pure, no UI) decides whether the line is `git <ref-taking command> … <word>`; `rankSuggestions`
  puts prefix matches first, then substring matches (max 8, the exact name is dropped); `insertion` is what to type:
  the remainder when the case matches, else backspaces + the full name.
- Names (local, remote as `<remote>/<branch>`, tags) come from the existing `getLocalBranches`/`getRemotes`/`getTags`,
  loaded on the first suggestion and dropped when `path` or `refsKey` (RepoView's `graphKey`) changes, so a branch
  made in the terminal is known after the watcher's reload. These count as sidebar reads for the busy bar.
- While the list shows, `TerminalPanel`'s custom key handler takes Tab / ↑ / ↓ / Esc (and calls `preventDefault`
  on Tab, else the webview moves focus). The list is `position: fixed` in viewport pixels from the `.xterm-screen`
  rect and the cursor cell, re-anchored on `onCursorMove` (the echo arrives after `onData`).
- There is no frontend test runner; the pure functions were checked with a throwaway script (TypeScript transpiled
  in node). The popup placement and key handling are untested in the running app.

## Git command list
- Scope decision: real `git` processes only. Every spawn goes through `toolbar::sync::run_git_inner`, which wraps the
  old body (`run_git_process`) in `gitlog::begin`/`finish`, so a new call site is logged without doing anything. Operations
  done with libgit2 (stage, checkout, branches, stashes, tags) run no command; showing "equivalent" commands for them
  was offered and declined (they would be invented text to keep in sync by hand).
- The log lives in the backend (`LOG`, 500 entries, output cut to 2000 chars) and records whether or not the panel is open;
  `GitLogPanel` is mounted only while open, loads `get_git_log` and merges `git-log` events by `id`
  (start event, then the same id again with the result), so nothing is lost or doubled in the gap.
- It is global, not per repository: auto-fetch, `ls-files` (Files tab) and `blame` hints show up too, which is
  honest but busy; the filter field is the answer to noise. A flag for "background" calls was dropped because
  `run_git_with` serves user actions (commit) as well as background work.
- `GitLogPanel` is a sibling of `TerminalPanel` at the bottom of `RepoView`; open state `gitLog.open` is persisted because
  a tab switch remounts `RepoView`. Resizing uses `Splitter` with the panel height as the state (drag up = taller).
