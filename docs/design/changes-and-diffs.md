# Changes, diffs and stashes: design notes

Contributor notes moved out of CLAUDE.md: how it is built and why. What the user sees is described in `docs/features/`. Covers amend, working-tree diffs, stash, the Changes file menu, file history, hunks, line staging and the diff view chrome.

- **Amend** switch pre-fills the last message, keeps the author, makes you the committer, works with
  nothing staged, warns when the commit is already pushed.

- **Working-tree diffs** (click a file in Changes): *unstaged* = index vs file on disk (untracked files show as
  all additions), *staged* = HEAD vs index (on an unborn branch everything staged is an addition). Staging
  buttons on a row stop propagation so they don't also open the diff. Committing closes an open working diff.

- **Stash** (behaviour: `docs/features/stashes.md`) moves all uncommitted changes, **including untracked files** (not ignored ones). `StashDialog` is opened from `Changes` and from the
  title bar button, which is owned by `RepoView` (it counts the changes with `useWorkingChangeCount`). Stashes appear in the graph (hollow node off the base commit,
  `stash@{n}` chip, row `isStash`) and in the `Stashes` section; clicking either opens the stash's `CommitDetail`, whose footer (`cd-footer`) has Pop.
  **Pop and Apply** (`apply_stash(repo, id, remove)`; pop is `apply_stash(.., true)`): apply, check for conflicts, then drop (pop only); re-stages what was staged.
  They require a clean working directory (disabled in the UI via `useWorkingChangeCount`, enforced in Rust) so a conflicting apply can be undone exactly
  (`restore_clean`) with the stash kept. **Delete stash** (`drop_stash_cmd`) just calls `stash_drop` by commit id and needs no clean directory. **Delete all**
  (`drop_all_stashes_cmd`) drops position 0 repeatedly: one undo step (`Kind::Keep`, restored with `git stash store`); `onAllStashesDropped` closes the right panel if
  it showed one of them. A graph row with `isStash` shows only Apply / Pop / Delete stash (`RepoView.stashAction`; `hasChanges` disables Apply and Pop).
- **File context menu** (right-click a row in `Changes`): Unstaged = Stage / Discard / Stash, Staged = Unstage / Stash. Discard
  (`discard_paths`) restores from the index (so staged edits survive) or deletes an untracked file, only for paths that
  really have unstaged changes, never for conflicted files, always after a confirmation. Single-file **Stash**
  runs `git --literal-pathspecs stash push --include-untracked -- <paths>`: libgit2's path-limited `stash_save_ext` also
  cleans the files that were NOT selected, and git2 cannot set its message, so don't use it.
- **File history** (behaviour: `docs/features/commit-details.md`; not for new files, stash files or during a rebase): `get_file_history(path, file, from)` takes only a hex id,
  validated with `Oid::from_str` before it reaches git. `RepoView` keeps `history: { file, from, entry }`. `FileHistory` (centre, `center-pane`, stays mounted but
  `hidden` while a commit's diff is open so the scroll position survives; its Escape is off then via `active`) lists `get_file_history`; a click sets `entry` and shows
  the existing `FileDiff` with `source` = that commit and the path *as it was in that commit* (+ `oldPath` for renames), with `backLabel`/`backHint` overriding the back
  button. Uses system git (`--follow` is not available in libgit2) with `--literal-pathspecs`; merge commits without a change to the file simply don't appear.
  Selecting a commit/working file or starting a rebase closes it.
- **Hunks** (behaviour: `docs/features/commit-details.md`): a hunk is a maximal run of added/removed lines (`DiffLine::block`, numbered in file order and identical in
  full-file and context-only views); `FileDiff::blocks` holds one FNV fingerprint per hunk. Stage / discard / unstage (`stage_hunk_cmd` / `discard_hunk_cmd` /
  `unstage_hunk_cmd`) re-diff and refuse with a "changed since this diff was shown" error when the fingerprint no longer matches. They do not build patches:
  `changes/hunks.rs` rebuilds the index blob (stage) or the file on disk (discard) line by line from the full-file diff using each line's raw bytes, so CRLF files
  and a missing final newline survive (discard re-inserts restored lines with the file's own EOL). Staging a deletion removes the index entry; discarding one
  checks the file out of the index. Unstaging rebuilds the index blob from the HEAD-to-index diff with that hunk reverted to HEAD's lines; if the file is not in
  HEAD (new file, or a branch without commits) and nothing is left, the index entry is removed, and a staged deletion is re-added from HEAD. The file on disk is
  never touched. Only the unstaged view of a tracked, non-conflicted, non-binary file offers stage/discard; discarding a staged hunk is not offered.
- **Stage line(s)** (`stage_lines_cmd` in `changes/hunks.rs`, right-click an add/del line in the unstaged diff of a tracked file; `FileDiff` `lineMenu`): `stage_selected` is `stage_hunk`'s code with a target line: the request is a list of `LineRef`s: hunk (`block` + fingerprint) and the line's position among that hunk's add/del lines (`line`, identical in full-file and changes-only views, so the UI needs no mapping); lines may come from several hunks, each hunk's fingerprint is checked (`locate_blocks`, `hunks_of`) and `resolve_lines` turns them into positions in the full-file diff (`Pick::Lines`; `Pick::Hunk` for whole hunks). The index blob is rebuilt from the full-file diff: context kept, a selected `del` dropped, a selected `add` taken in, every other change left as the index has it (so a removed line staged on its own leaves the replacement unstaged and vice versa). A wrong fingerprint or an offset past the hunk is refused as stale. One undo step (`Kind::Index`). **Discard line(s)** (`discard_lines_cmd`, `Kind::Full`, behind one confirmation) is `discard_hunk`'s code with the same target line (`discard_pick`, `Pick` shared with staging): an `add` line is left out of the file on disk, a `del` line is re-inserted from the index with the file's own EOL; a deleted file gets recreated with just that line. The index is never touched. The staged diff has no line menu yet. **Multi-line selection uses the browser's own text selection**, not a custom one: on right-click `FileDiff.selectedRows()` clips the selection `Range` to each rendered `.dl[data-row]`'s `.tx` and counts the rows with text inside (so a selection that only touches a line's end or the next line's start doesn't count); if the right-clicked line is among ≥ 2 selected changed lines the menu acts on all of them ("Stage N lines"), otherwise on that one. Rows are keyed by their absolute index so their DOM nodes (and the selection) survive scrolling, but the list is virtualised: only rows in view ± 20 exist, so a selection can't reach beyond them.

- In the full-file view `FileDiff` draws change markers over the vertical scrollbar (`.fd-marks`: runs of add/del rows as percentages of the
  virtual list, `pointer-events: none`, narrower than the 10px scrollbar so the thumb stays visible; shown only when the list scrolls).
- `FileDiff` header ▲ / ▼ buttons jump between hunk heading rows (smooth scroll, two rows of context above; "where we are" = scrollTop + 2 rows, so
  repeated clicks always advance; disabled at the first/last hunk and when the list can't scroll further). No keyboard shortcut: ↑/↓ already move
  between files while a diff is open.
- Commit detail diffs are against the **first parent**; renames detected; 2000-file and 20 000-line caps;
  binary/over-5 MB files are not previewed. The diff view has three modes (`FileDiff` `mode`, header buttons, persisted as `diff.mode`): *Full file* (default), *Changes* (the backend's context-only diff) and *Side by side*. **Side by side** uses the full-file diff (`full = mode !== "hunks"`) and only changes how `rows` are built: `del`/`add` runs are flushed as `pair` rows (the i-th removed line next to the i-th added one, blanks opposite the longer run), context lines appear on both sides; hunk headings stay full-width rows, so hunk buttons, ▲/▼ navigation (`hunkTops`) and the virtual list (fixed `ROW_H`) are unchanged. Each half is `flex: 1 0 auto` with a min-width of the longest line (`--cols`, capped at 300) so the row scrolls as one. Line stage/discard work per half (`LineTarget.side`; `targetOf(row, side)`) but a single line at a time: the text-selection multi-select reads one `.tx` per row and would run across both halves. Scrollbar marks use `mod` for a removed line facing an added one.
