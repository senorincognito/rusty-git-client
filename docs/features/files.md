# Files

- The left panel's **Files** tab lists every file of the working directory as a tree of folders: tracked files plus untracked ones that are not ignored
  (files deleted on disk are left out). Folders start closed; click one to open or close it. Each folder shows how many files it holds.
- The filter field above the lists filters this one too (all words must occur in the path, case-insensitively); while a filter is active every folder is open.
- Files with uncommitted changes are highlighted (green new, amber modified, red conflicted) and so are the folders containing them.
- Right-click a file > **Show git history** lists every commit that changed it (following renames); click a commit to see that commit's change to the file. **Back** returns to the file.
- Click a file to see its content in the centre with line numbers.
- Click a line to pick it: the commit that last changed it appears after the end of the line (short SHA, author, commit message); click it again to let go. Lines not committed yet say so. Two small buttons follow the SHA: one copies the full SHA to the clipboard (a check mark confirms), the other switches to the Branches tab and shows that commit in the graph, selected and scrolled into view, with its files in the right panel. In the editor the line with the cursor gets the hint while the file has no unsaved changes.
- **Shift+click** another line extends the pick to a range. **Send to terminal** (button above the file, or right-click) types a reference such as `@src/app.ts#L12-20` into the terminal's prompt without pressing Enter, opening the terminal if needed, so an AI tool running there (Claude Code, for example) can be pointed at exactly that code. **Copy reference** puts the same text on the clipboard. With nothing picked the reference is just the file. Both are off while the file has unsaved changes.
- **Edit** turns the view into a plain text editor; **Save** (or Ctrl/Cmd+S) writes the file, **Cancel** drops the changes. Line endings of the file are kept. Opening another file while there are unsaved changes asks first; switching tabs keeps them.
- **New file…** (the "⋯" menu of the Files headline, or right-click a folder for one inside it) asks for a path relative to the repository (missing folders are created) and creates an empty file.
- Right-click a file > **Delete file** (or the **Delete** button above the content) removes it from the working directory after a confirmation. Edits, new files and deletions are plain file changes: they show up in the Changes panel to be staged and committed, and Undo does not cover them.
- While the Files tab is open the **right panel is hidden** (the Changes panel keeps what you typed). The graph and any open diff are put away and come back with the other tabs;
  starting a search (**Search** or Ctrl/Cmd+F) switches back to the Branches tab.
- The list and the open file refresh when the window regains focus, so edits made in another program show up.
- Binary files and files over 2 MB are not shown (a note says so) and can't be edited.
