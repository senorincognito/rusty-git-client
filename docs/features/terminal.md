# Terminal

- Toggle a real terminal (PowerShell on Windows, your login shell on macOS) in the
  repository folder with the **>_ Terminal** button or the Ctrl + Backquote shortcut. Resizable. Each working directory has its own
  session, which keeps running while the panel is hidden and while you look at another tab (a long build or an
  agent carries on); showing it again replays what it printed. Closing the tab ends the session (after asking while an
  agent runs in it).
- Anything you run there (commits, checkouts, ...) shows up in the UI through live reload.
- **Branch and tag suggestions**: while you type a git command that takes a ref (`checkout`, `switch`, `merge`,
  `rebase`, `cherry-pick`, `branch`, `diff`, `log`, `show`, `reset`, `revert`, `tag`, `push`, `pull`, `merge-base`),
  a list of matching local branches, remote branches (`origin/main`) and tags appears at the cursor. `Tab` inserts
  the highlighted name, `↑` / `↓` choose, `Esc` closes it, a click picks one. It needs a first letter of the name,
  skips options (`-b`, `--force`) and the name of a branch you are creating (`checkout -b`), and completes the
  part after `..` in ranges. Turn it off in [Settings](settings.md).
- The suggestions only follow what you type. After history (`↑`), cursor keys, the shell's own `Tab` completion or
  a multi-line paste the app can't see the line any more, so there are no suggestions until the next `Enter` or
  `Ctrl` + `C`. Full-screen programs (less, vim) never get them.

Back to the [feature index](../../FEATURES.md).
