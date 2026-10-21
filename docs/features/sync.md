# Fetch, pull and push

- Title-bar buttons run the system `git`, so your credential helper and SSH setup apply.
- Fetch updates all remotes (with prune). Pull fast-forwards, so it never creates a surprise
  merge. When the branches have diverged, a dialog lists the commits on each side and offers
  **Merge** (the default: keeps every commit, adds a merge commit) or **Rebase** (replays your
  commits on top of the upstream, giving them new ids). Uncommitted changes are set aside and
  restored. If there are conflicts, the pull is cancelled with the file names and the repository is
  left exactly as it was (resolve in the terminal for now). Push publishes a new branch to `origin` and sets its upstream.
- **Sign-in prompt** (macOS): when a fetch, pull or push needs a username, password, token or SSH passphrase and no credential
  helper supplies it, a dialog asks for it (for GitHub and similar over HTTPS enter a personal access token as the password).
  Cancel aborts the operation. If Git has a credential helper (e.g. the macOS keychain) it stores the login afterwards. Windows
  uses Git Credential Manager's own window. Auto-fetch never asks; it pauses on missing credentials as before.
- **Push on a diverged branch**: when your branch and its upstream each have commits the other lacks, Push does not just fail. A dialog lists both
  sides and offers **Force push** (with lease; the remote-only commits are discarded there) or **Cancel**, which is the default and changes nothing. To keep the
  remote commits, pull (merge or rebase) first. The check uses the last fetched state; fetch first if you suspect the remote moved.
- **Auto-fetch** (on by default, every 3 minutes): fetches in the background while the window
  is focused, and right away when you come back to a stale repo. It never overlaps another
  git operation, stays silent (a small spinner shows while it runs), backs off when the
  remote is unreachable, and pauses instead of retrying when credentials are needed. Switch it
  off or pick 1 / 3 / 5 / 10 minutes from the ▾ next to Fetch (or right-click Fetch).
- **Pull options**: click the small ▾ beside Pull (or right-click it) to pull with an explicit
  strategy, **Pull (merge)** or **Pull (rebase)**, without the diverged-branches dialog.
- **Force push**: right-click the Push button, or click the small ▾ beside it. It asks for
  confirmation first (and says how many remote commits will be discarded), then runs
  `git push --force-with-lease`, which is refused if the remote moved since your last fetch.
  Use it after amending or renaming a commit that was already pushed.
- Buttons show `↓n` / `↑n` counts and are disabled with a tooltip when they can't work.

Back to the [feature index](../../FEATURES.md).
