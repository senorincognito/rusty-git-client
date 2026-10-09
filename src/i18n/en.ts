// Every text the user reads, grouped by the part of the app it belongs to. Plain texts are strings;
// texts with numbers or names in them are functions. "{name}" placeholders are filled with markup by
// `fill` (see fill.tsx). Backend error messages come from Rust and are shown as they are.

/** "1 commit", "3 commits". */
const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

export const en = {
  common: {
    cancel: "Cancel",
    delete: "Delete",
    loading: "Loading…",
  },

  auth: {
    title: "Sign in",
    answerLabel: "Answer",
    tokenHint:
      "For GitHub, GitLab and similar hosts over HTTPS, use a personal access token here, not your account password.",
    submit: "Continue",
  },

  welcome: {
    title: "Rusty Git Client",
    open: "Open repository…",
    pickerTitle: "Open repository",
    recent: "Recent",
    noRecent: "No recent repositories.",
    removeRecent: "Remove from list",
  },

  /** Names of the one-letter file status badges (A, M, D, ...). */
  fileStatus: {
    new: "Added",
    modified: "Modified",
    deleted: "Deleted",
    typechange: "Type changed",
    conflicted: "Conflicted",
    renamed: "Renamed",
    copied: "Copied",
  },

  splitter: {
    label: "Resize section",
    hint: "Drag to resize, double-click to reset",
  },

  resizePanel: {
    label: "Resize panel",
    hint: "Drag to resize, double-click to reset",
  },

  stashDialog: {
    title: "Stash changes",
    explain: (files: number) =>
      `${files === 1 ? "The 1 changed file" : `All ${files} changed files`}, including untracked files, will be moved into a new stash and your working directory will be clean.`,
    messagePlaceholder: "Message (optional)",
    messageLabel: "Stash message",
    hint: "Select the stash in the graph or the Stashes list and press Pop to bring the changes back.",
    submit: "Stash",
  },

  pullDialog: {
    title: "Branches have diverged",
    lead: "{branch} and {upstream} each have commits the other doesn't, so the pull can't simply fast-forward. Choose how to combine them.",
    onlyLocal: "↑ Only on your branch",
    onlyRemote: (upstream: string) => `↓ Only on ${upstream}`,
    more: (n: number) => `… and ${n} more`,
    hint: "{merge} keeps all commits as they are and adds a merge commit (the safest choice). {rebase} replays your commits on top of {upstream}, which gives them new ids. If there are conflicts, the pull is cancelled and nothing is changed. Uncommitted changes are set aside and restored.",
    merge: "Merge",
    rebase: "Rebase",
  },

  pushDialog: {
    title: "Branches have diverged",
    lead: "{branch} and {upstream} each have commits the other doesn't, so a normal push would be rejected.",
    hint: "{force} overwrites {upstream} with your branch: {discarded} It is refused if the remote changed since your last fetch. Cancel leaves everything as it is; to keep the remote commits, pull first (merge or rebase) and push afterwards.",
    discarded: (n: number, upstream: string) =>
      `the ${plural(n, "commit")} only on ${upstream} will be discarded there.`,
    forcePush: "Force push",
  },

  undo: {
    undo: "↶ Undo",
    redo: "↷ Redo",
    undoTitle: "Undo",
    redoTitle: "Redo",
    nothingToUndo: "Nothing to undo",
    nothingToRedo: "Nothing to redo",
    undoHint: (label: string) => `Undo: ${label} (Ctrl/Cmd+Z)`,
    redoHint: (label: string) => `Redo: ${label} (Ctrl/Cmd+Shift+Z)`,
  },

  fileHistory: {
    title: "File history",
    titleFrom: (shortId: string) => `File history up to ${shortId}`,
    back: "← Back to graph",
    backHint: "Back to the commit graph (Esc)",
    backToList: "← Back to file history",
    backToListHint: "Back to the list of commits for this file (Esc)",
    count: (n: number) => plural(n, "commit"),
    none: "No commit has changed this file yet.",
    capped: "Only the newest 500 commits are listed.",
    openHint: (shortId: string) => `Show what ${shortId} changed in this file`,
    renamedFrom: (old: string) => `renamed from ${old}`,
  },

  sidebar: {
    filterPlaceholder: "Filter branches and stashes…",
    filterLabel: "Filter branches and stashes",
    clearFilter: "Clear the filter",
    clearFilterHint: "Clear the filter (Esc)",
    branchNameLabel: "New branch name",
  },

  localBranches: {
    title: "Local branches",
    actions: "Branch actions",
    none: "No branches yet.",
    noMatch: "No branch matches the filter.",
    fastForward: (current: string, name: string) => `Fast-forward ${current} to ${name}`,
    fastForwardHint: "Move the checked-out branch forward to this branch (only possible when this branch is ahead of it)",
    fastForwardCurrent: "This branch is checked out",
    merge: (name: string, current: string) => `Merge ${name} into ${current}`,
    mergeHint: "Merge this branch into the checked-out branch (asks first)",
    mergeCurrent: "This branch is checked out: pick another branch to merge into it",
    mergeDetached: "HEAD is detached: check out a branch to merge into it",
    fastForwardDetached: "HEAD is detached: check out a branch to fast-forward it",
    folderCount: (n: number) => plural(n, "branch", "branches"),
    current: (name: string) => `${name} (current)`,
    checkoutHint: (name: string) => `Double-click to check out ${name}`,
    newBranch: "New branch…",
    newBranchHint: "Create a branch at the current commit and check it out",
    rename: "Rename branch",
    renameCurrent: "The checked-out branch can't be renamed. Switch to another branch first.",
    delete: "Delete branch",
    deleteCurrent: "The checked-out branch can't be deleted. Switch to another branch first.",
    deleteTitle: "Delete branch",
    deleteConfirm: (name: string) => `Delete branch "${name}"?`,
    deleteUnmerged: (name: string, commits: number) =>
      `"${name}" has ${plural(commits, "commit")} that are not merged into the current branch or pushed to its upstream. They will be hard to recover once the branch is gone.\n\nDelete "${name}" anyway?`,
  },

  remotes: {
    title: "Remotes",
    actions: "Remote actions",
    noMatch: "No remote branch matches the filter.",
    noBranches: "No remote branches yet. Fetch to load them.",
    target: "target",
    targetHint: "New branches are pushed to this remote",
    fetchFailed: "Fetching failed",
    fetchFailedHint: (message: string) => `Fetching failed: ${message}`,
    urlLabel: "Remote URL",
    addRemote: "Add remote…",
    addRemoteHint: "Add another remote repository",
    addRemoteUseForm: "Use the form below",
    targetRemote: "Target remote",
    targetRemoteHint: "The remote that Push publishes new branches to",
    setTarget: "Set as target",
    alreadyTarget: "New branches are already pushed to this remote",
    setTargetHint: "Push new branches to this remote (Push on a branch that has no upstream yet)",
    editUrl: "Edit URL",
    remove: "Remove remote",
    removeHint: "Remove it from this repository's settings (nothing on the server is touched; asks first)",
    removeTitle: "Remove remote",
    removeOk: "Remove",
    removeConfirm: (name: string, url: string, remoteBranches: number, trackingBranches: number) =>
      [
        `Remove the remote "${name}" (${url})?`,
        "",
        `Only this repository's settings change: the remote and its ${plural(remoteBranches, "remote-tracking branch", "remote-tracking branches")} are removed here. Nothing on the server is touched.`,
        ...(trackingBranches > 0
          ? ["", `${plural(trackingBranches, "local branch", "local branches")} track${trackingBranches === 1 ? "s" : ""} it and will no longer have an upstream.`]
          : []),
      ].join("\n"),
    checkout: "Check out",
    merge: (full: string) => `Merge ${full} into the current branch`,
    mergeHint: "Merge this remote branch into the checked-out branch (asks first)",
    checkoutHint: (full: string) => `Double-click to check out ${full} as a local branch`,
    checkingOut: (full: string) => `Checking out ${full}…`,
    renameBranch: "Rename remote branch",
    deleteBranch: "Delete remote branch",
    upstreamOfHead: "This is the upstream of the checked-out branch. Switch branches first.",
    deleteBranchConfirm: (full: string, unique: number) =>
      `Delete "${full}" from the remote? The branch is removed on the server for everyone who uses it.` +
      (unique > 0
        ? `\n\n${plural(unique, "commit")} on it exist nowhere else: not in your current branch or any local branch.`
        : ""),
    deleting: (full: string) => `Deleting ${full}…`,
    renameBranchConfirm: (from: string, to: string) =>
      `Rename "${from}" to "${to}"?\n\nThis creates ${to} and deletes ${from} on the server, for everyone who uses it. Local branches that track ${from} will be pointed at the new name.\n\nIf somebody pushed to ${from} since your last fetch, the rename is refused.`,
    renameOk: "Rename",
    renaming: (full: string) => `Renaming ${full}…`,
  },

  addRemote: {
    first: "No remote yet. Add the URL of the remote repository.",
    another: "Add another remote repository.",
    namePlaceholder: "Name, e.g. origin or upstream",
    nameLabel: "Remote name",
    urlPlaceholder: "https://github.com/user/repo.git",
    urlLabel: "Remote URL",
    submit: "Add remote",
  },

  stashes: {
    title: "Stashes",
    none: "No stashes.",
    noMatch: "No stash matches the filter.",
    apply: "Apply stash",
    applyHint: "Apply this stash to the working directory and keep it in the list",
    pop: "Pop stash",
    popNeedsClean: "Commit or stash your uncommitted changes first",
    popHint: "Apply this stash to the working directory and remove it from the list",
    delete: "Delete stash",
    deleteHint: "Remove this stash without applying it (asks first)",
    deleteConfirm: (ref: string, message: string) =>
      `Delete ${ref} "${message}"?\n\nIts changes are not applied, and they are lost: git has no undo for this.`,
  },

  newBranch: {
    button: "⑂ Branch",
    nameLabel: "New branch name",
    namePlaceholder: "feature/my-change",
    hint: "Created from the current commit and checked out.",
    submit: "Create & checkout",
  },

  sync: {
    fetch: "Fetch",
    pull: "Pull",
    push: "Push",
    forcePush: "Force push",
    complete: (op: string) => `${op} complete`,
    pullComplete: "Pull complete",
    minutes: (n: number) => `${plural(n, "minute")}`,
    autoFetchFetching: "Fetching in the background…",
    autoFetchAuth: "Auto-fetch is paused: the remote needs you to sign in. A successful manual fetch resumes it.",
    autoFetchWaiting: "Auto-fetch can't reach the remote right now. It will retry with a longer delay.",
    forcePushConfirm: (branch: string | null, upstream: string, discarded: number) =>
      [
        `Force push "${branch}" to ${upstream}?`,
        "",
        "This overwrites the remote branch with your local history.",
        ...(discarded > 0
          ? [`${plural(discarded, "commit")} on ${upstream} that are not in your branch will be discarded there.`]
          : []),
        "",
        "The push is refused if the remote changed since your last fetch.",
      ].join("\n"),
    autoFetch: "Auto-fetch",
    autoFetchHint: "Fetch in the background while this window is focused",
    every: (interval: string) => `Every ${interval}`,
    noRemotes: "No remotes configured",
    checkOutBranch: "Check out a branch first",
    noUpstreamToPull: "No upstream branch to pull from",
    pullMerge: "Pull (merge)",
    pullMergeHint: (upstream: string | null | undefined) =>
      `Fetch and merge ${upstream} into this branch. A merge commit is added if both have new commits.`,
    pullRebase: "Pull (rebase)",
    pullRebaseHint: (upstream: string | null | undefined) =>
      `Fetch and replay your commits on top of ${upstream}. Your commits get new ids.`,
    notPushedYet: "This branch has not been pushed yet. Use Push first.",
    forcePushHint: (upstream: string) => `Overwrite ${upstream} with your branch (asks first)`,
    fetchAll: "Fetch all remotes",
    autoFetchEvery: (interval: string) => ` · auto-fetch every ${interval}`,
    lastAt: (time: string) => `, last at ${time}`,
    autoFetchOff: " · auto-fetch is off",
    autoFetchSettings: "Auto-fetch settings",
    morePull: "More pull options",
    morePush: "More push options",
    noUpstream: "No upstream branch",
    diverged: (upstream: string, ahead: number, behind: number) =>
      `Diverged from ${upstream} (↑${ahead} ↓${behind}): choose merge or rebase`,
    pullFrom: (upstream: string) => `Pull (fast-forward) from ${upstream}`,
    pushTo: (upstream: string) => `Push to ${upstream}`,
    publish: "Publish this branch",
    newBadge: "new",
    dismiss: "Click to dismiss",
  },

  changes: {
    unstaged: "Unstaged",
    staged: "Staged",
    stage: "Stage",
    unstage: "Unstage",
    actionAll: (action: string) => `${action} all`,
    amend: "Amend previous commit",
    amendHint: (shortId: string) => `Replace the last commit (${shortId}) instead of creating a new one`,
    nothingToAmend: "There is no commit to amend yet",
    amendPushed: "This commit is already pushed. Amending it rewrites history, so it will need a force push.",
    messagePlaceholder: "Commit message",
    stashAll: "Stash…",
    stashAllHint: "Move all uncommitted changes, including untracked files, into a stash",
    commit: "Commit",
    amendCommit: "Amend commit",
    count: (n: number) => ` (${n})`,
    // Right-click menu on a file
    stash: "Stash",
    fileHistory: "File history",
    fileHistoryHint: "List every commit that changed this file",
    fileHistoryNew: "This file is new: it has no history yet",
    stashFileHint: "Move this file's uncommitted changes (staged and unstaged) into a new stash; everything else stays",
    deleteFile: "Delete file…",
    restoreFile: "Restore file…",
    discard: "Discard changes…",
    conflicted: "This file has merge conflicts",
    discardHint: "Throw away your unstaged changes to this file (asks first)",
    discardTitle: "Discard changes",
    discardOk: "Discard",
    restoreOk: "Restore",
    deleteUntrackedConfirm: (path: string) => `Delete the untracked file "${path}"? It is not in git, so it cannot be recovered.`,
    restoreConfirm: (path: string) => `Restore "${path}", which you deleted?`,
    discardConfirm: (path: string, alsoStaged: boolean) =>
      `Discard your changes to "${path}"? They are lost for good (git cannot recover them).` +
      (alsoStaged ? " Changes that are staged stay staged." : ""),
  },

  commitDetail: {
    fileHistory: "File history",
    fileHistoryHint: "List the commits that changed this file, starting at this commit",
    fileHistoryStash: "A stash's files have no history of their own",
    title: "Commit",
    close: "Close commit details",
    workingChanges: (n: number) => `${plural(n, "file change")} in working directory`,
    viewChanges: "View changes",
    stashContents: "{stash}: every uncommitted change that was saved here, including untracked files",
    madeOn: (parent: string | undefined) => `Made on ${parent ?? "an unknown commit"}`,
    root: "Root commit",
    parents: (ids: string[]) => `Parent${ids.length === 1 ? "" : "s"}: ${ids.join(", ")}`,
    againstFirstParent: " · changes shown against the first parent",
    changedFiles: "Changed files",
    noFiles: "This commit changes no files.",
    truncated: (shown: number, total: number) => `Showing the first ${shown} of ${total} files.`,
    popNeedsClean: "Commit or stash your uncommitted changes first: a stash can only be popped onto a clean working directory.",
    popHint: "Apply this stash to the working directory and remove it from the list",
    pop: "Pop stash",
    popping: "Popping…",
  },

  rename: {
    title: "Rename commit",
    messageLabel: "Commit message",
    pushed: "This commit is already pushed. Renaming rewrites history, so it will need a force push.",
    later: (n: number) => `${plural(n, "later commit")} on this branch will be rewritten too (new ids, same content).`,
    update: "Update",
  },

  diff: {
    prevChange: "Previous change",
    nextChange: "Next change",
    changePosition: (at: number, total: number) => `${at} / ${total}`,
    closeHint: "Close the diff (Esc)",
    backHint: "Back to the commit graph (Esc)",
    back: "← Back",
    backToGraph: "← Back to graph",
    renamedFrom: (path: string) => `renamed from ${path}`,
    staged: "staged",
    unstaged: "unstaged",
    firstLines: (n: number) => ` · first ${n} lines shown`,
    fullFileHint: "Show the whole file, or only the changed parts with 3 lines of context",
    fullFile: "Full file",
    binary: "Binary or very large file: no preview.",
    noWorkingChanges: (kind: string) => `This file has no ${kind} changes (any more).`,
    noContentChanges: "No content changes in this file (for example, only its mode changed).",
    hunk: (n: number, total: number) => `Hunk ${n} of ${total} · `,
    unstageHunk: "Unstage hunk",
    unstageHunkHint: "Take this hunk back out of the staging area (the file on disk is not changed)",
    stageHunk: "Stage hunk",
    stageHunkHint: "Put this hunk into the staging area",
    discardHunk: "Discard hunk",
    discardHunkHint: "Remove this hunk from the file (cannot be undone)",
    discardHunkConfirm: (adds: number, dels: number, path: string) =>
      `Discard this hunk (+${adds} -${dels}) from ${path}?\n\nThe lines are removed from the file and can't be recovered.`,
    discardOk: "Discard",
  },

  graph: {
    noCommits: "No commits yet.",
    checkout: (name: string) => `Check out ${name}`,
    checkoutGroup: "Check out",
    checkoutHint: (name: string) => `Switch to ${name}. A remote branch becomes a local branch that tracks it. Double-click the commit to do the same.`,
    rename: "Rename commit",
    renameStash: "A stash can't be renamed",
    renameNotOnBranch: "Only commits on the current branch can be renamed",
    rebase: "Interactive rebase…",
    rebaseHint: "Edit the messages of all commits after this one on the current branch",
    rebaseStash: "A stash can't be a rebase base",
    rebaseNotOnBranch: "Only commits on the current branch can be a rebase base",
    rebaseNothingAfter: "There are no commits after this one",
    drop: "Drop commit",
    dropHint: "Remove this commit and its changes from the branch (asks first)",
    dropStash: "A stash can't be dropped here",
    dropNotOnBranch: "Only commits on the current branch can be dropped",
    dropFirst: "The first commit of a branch can't be dropped",
    reset: "Reset to this commit",
    resetStash: "A stash can't be a reset target",
    fastForward: "Fast-forward to this commit",
    fastForwardHint: "Move the checked-out branch forward to this commit (only possible when this commit is a descendant of it)",
    fastForwardContained: "The checked-out branch already contains this commit",
    fastForwardStash: "A stash can't be fast-forwarded to",
    resetHint: "Move the branch to this commit; choose what happens to the staging area and your files",
    resetSoft: "Soft – keep changes staged",
    resetSoftHint: "Move the branch here; the staging area and your files are not touched (asks first)",
    resetMixed: "Mixed – keep changes unstaged",
    resetMixedHint: "Move the branch here and reset the staging area; your files are not touched (asks first)",
    resetHard: "Hard – discard changes",
    resetHardHint: "Move the branch here and reset the staging area and your files; uncommitted changes are lost (asks first)",
  },

  repo: {
    back: "← Repositories",
    detached: "detached @ ",
    noCommits: "(no commits yet)",
    stash: "Stash",
    stashNothing: "No uncommitted changes to stash",
    terminal: ">_ Terminal",
    terminalHint: "Toggle terminal (Ctrl+`)",
    dropTitle: "Drop commit",
    checkoutTitle: "Check out",
    stashTitle: "Stash",
    fastForwardTitle: "Fast-forward",
    mergeTitle: "Merge",
    mergeConfirm: (source: string) =>
      `Merge "${source}" into the checked-out branch?\n\nA merge commit is created unless the branch can simply be fast-forwarded. Uncommitted changes are set aside and restored. If there are conflicts the merge is cancelled and nothing is changed.`,
    mergeOk: "Merge",
    mergeUpToDate: (source: string) => `The checked-out branch already contains "${source}". Nothing to merge.`,
    dropConfirm: (info: {
      shortId: string;
      summary: string;
      laterCommits: number;
      isMerge: boolean;
      pushed: boolean;
      laterPushed: number;
    }) =>
      [
        `Drop commit ${info.shortId} "${info.summary}"?`,
        "",
        "The commit is removed from the current branch, and its changes are removed from your working directory.",
        ...(info.laterCommits > 0
          ? [
              "",
              `The ${plural(info.laterCommits, "later commit")} on this branch will be re-created on top of its parent (new ids, same changes, authors and messages; signatures are not kept). If one of them depends on the dropped commit, the drop is cancelled and nothing is changed.`,
            ]
          : []),
        ...(info.isMerge ? ["", "This is a merge commit: the branch goes back to its first parent."] : []),
        ...(info.pushed || info.laterPushed > 0
          ? ["", "Part of this history is already pushed. Dropping it rewrites that history, so it will need a force push."]
          : []),
        "",
        "Other branches, tags and stashes that point at the old commits keep the old history.",
        "",
        "Git keeps the old commits in its reflog for a while, so they can still be recovered with git reflog.",
      ].join("\n"),
    resetTitle: (mode: string) => `Reset (${mode})`,
    resetOk: (mode: string) => `Reset ${mode}`,
  },

  /** The confirmation text of a reset (see features/repo/describeReset.ts). */
  reset: {
    branch: (name: string) => `branch "${name}"`,
    detachedHead: "the detached HEAD",
    sameCommit: (target: string, where: string) => `Reset to ${target}, the commit ${where} is already on. No commits move.`,
    move: (where: string, from: string, target: string) => `Move ${where} from ${from} to ${target}.`,
    removed: (n: number, where: string) => `${plural(n, "commit")} will no longer be on ${where}:`,
    more: (n: number) => `  … and ${n} more`,
    forward: (where: string, n: number) => `${where} moves forward by ${plural(n, "commit")}.`,
    otherLine: "This commit is not part of the current history, so the branch jumps to a different line of history.",
    soft: "Soft reset: the staging area and your files are not touched.",
    softRemoved:
      "Soft reset: the staging area and your files are not touched. The changes of the removed commits show up as staged changes, ready to be committed again.",
    mixed:
      "Mixed reset: the staging area is reset to that commit; your files are not touched. Everything that was staged, and the changes of the removed commits, becomes unstaged changes (new files become untracked).",
    hard: "Hard reset: the staging area and your files are reset to that commit.",
    hardRemoved: " The changes of the removed commits disappear from your files.",
    hardLost: (n: number) => ` Your ${plural(n, "uncommitted file change")} will be lost for good (git cannot recover them).`,
    hardUntracked: " Untracked files are left alone.",
    pushed: (n: number) =>
      `${n} of the removed commits ${n === 1 ? "is" : "are"} already pushed: publishing the new position will need a force push.`,
    reflog: "The removed commits stay in git's reflog for a while, so they can be recovered with git reflog.",
  },

  terminal: {
    title: "Terminal",
    hide: "Hide terminal (Ctrl+`)",
    startFailed: (error: string) => `\r\nCould not start the terminal: ${error}\r\nPress any key to retry.\r\n`,
  },

  autoFetch: {
    signIn: "The remote needs you to sign in.",
    unreachable: "The remote can't be reached.",
  },

  rebase: {
    title: "Interactive rebase",
    onto: "onto {base}",
    count: (n: number) => ` · ${plural(n, "commit")}, newest first`,
    cancel: "Cancel rebase",
    start: "Start rebase",
    starting: "Rebasing…",
    pick: "pick",
    pickHint: "Keep the commit as it is",
    reword: "reword",
    rewordHint: "Keep the commit but change its message",
    rowHint: "Click to see the files this commit changes",
    menuReword: "Reword commit",
    menuSquash: "Squash commit",
    menuDrop: "Drop commit",
    menuMoveUp: "Move commit up",
    menuMoveDown: "Move commit down",
    moveUpHint: "Make this commit newer: swap it with the one above",
    moveDownHint: "Make this commit older: swap it with the one below",
    moveTopHint: "This is already the newest commit",
    moveBottomHint: "This is already the oldest commit",
    moved: "moved",
    movedHint: "Its place in the history changes",
    invalid: (shortId: string, reason: string) => `${shortId}: ${reason}`,
    squash: "squash",
    squashHint: "Meld the commit into the one before it (the next older one) and append its message",
    squashOldest: "The oldest commit has no commit before it to squash into",
    squashMerge: "A merge commit can't be squashed",
    squashInto: (shortId: string) => `into ${shortId}`,
    squashIntoDropped: "The commit before it is dropped, so there is nothing to squash into",
    drop: "drop",
    dropHint: "Remove the commit and its changes from the branch",
    dropSquashed: "Commits are squashed into this one: change those first",
    dropped: "dropped",
    replayNote: "Dropping or moving commits replays the later commits and resets your files: the working directory must be clean.",
    actionFor: (shortId: string) => `Action for ${shortId}`,
    editMessage: "Edit message",
    merge: "merge",
    pushed: "pushed",
    pushedHint: "Already pushed: changing it needs a force push",
    base: "base of the rebase, stays as it is",
    chooseAction: "Choose reword, squash or drop for the commits you want to change, or move them up and down (right-click).",
    summary: (reworded: number, squashed: number, dropped: number, reordered: boolean, alsoRewritten: number) =>
      [
        ...(reworded > 0 ? [`${plural(reworded, "commit")} reworded`] : []),
        ...(reordered ? ["order changed"] : []),
        ...(dropped > 0 ? [`${plural(dropped, "commit")} dropped`] : []),
        ...(squashed > 0 ? [`${plural(squashed, "commit")} squashed into the commit before`] : []),
        ...(alsoRewritten > 0 ? [`${plural(alsoRewritten, "later commit")} rewritten too (new ids, same content)`] : []),
      ].join("; ") + ".",
    pushedWarning: (n: number) =>
      `${plural(n, "rewritten commit")} ${n === 1 ? "is" : "are"} already pushed: this rewrites published history and will need a force push.`,
    rewordTitle: (shortId: string) => `Reword ${shortId}`,
    messageLabel: "Commit message",
    emptyMessage: "A commit message can't be empty.",
    rewordAppliesLater: "The new message is used when you start the rebase.",
    updateMessage: "Update message",
  },
};
