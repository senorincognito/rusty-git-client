# Git command list

- The **Commands** button in the title bar (next to Terminal) shows or hides a panel listing every call the app makes
  to the `git` program: time, the command with its arguments, the repository folder, and the result (✓ / ✗ with the
  duration; "running…" while it runs, so a slow fetch is visible). It is remembered across restarts.
- Click a line to see git's output (cut to 2000 characters); failed commands are red. The field filters by words in
  the command; **Clear** empties the list. Drag the top edge to resize (double-click resets).
- The list is kept (the last 500 calls) while the panel is closed, so opening it shows what already happened. It
  also shows what runs in the background, such as auto-fetch, `ls-files` for the Files tab and `blame` hints.
- Only real `git` processes are listed. Staging, unstaging, checkout, creating branches, stashing and similar
  actions are done inside the app (libgit2) and run no command, so they don't appear.
- Commands typed in the [terminal](terminal.md) are not listed (they are the shell's, not the app's).

Back to the [feature index](../../FEATURES.md).
