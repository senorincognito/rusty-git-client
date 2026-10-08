import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { ChangeKind } from "@/api/changes";
import type { CommitFile, HistoryEntry } from "@/api/commit";
import { confirmDialog, showError, showInfo } from "@/api/dialog";
import { applyStash, dropStash, popStash } from "@/api/stash";
import { dropLatestCommit, fastForward, getDropInfo, getResetInfo, mergeBranch, resetToCommit, type ResetMode } from "@/api/history";
import { openRepo, type RepoInfo } from "@/api/repo";
import { unwatchRepo, watchRepo } from "@/api/watch";
import { useWorkingChangeCount } from "@/hooks/useWorkingChangeCount";
import ResizablePanel from "@/components/ResizablePanel";
import Changes from "@/features/changes/Changes";
import CommitDetail from "@/features/commit/CommitDetail";
import StashDialog from "@/features/changes/StashDialog";
import FileHistory from "@/features/commit/FileHistory";
import FileDiff from "@/features/commit/FileDiff";
import Graph from "@/features/graph/Graph";
import InteractiveRebase from "@/features/rebase/InteractiveRebase";
import RenameCommit from "@/features/rename/RenameCommit";
import Sidebar from "@/features/sidebar/Sidebar";
import TerminalPanel from "@/features/terminal/TerminalPanel";
import BranchButton from "@/features/toolbar/BranchButton";
import SyncBar from "@/features/toolbar/SyncBar";
import { t } from "@/i18n";
import { describeReset } from "./describeReset";
import "./RepoView.scss";

/** The screen for an open repository: title bar, sidebar, graph, changes and terminal. */
export default function RepoView({
  repo,
  onRepoChange,
  onClose,
}: {
  repo: RepoInfo;
  onRepoChange: (repo: RepoInfo) => void;
  onClose: () => void;
}) {
  const [graphKey, setGraphKey] = useState(0);
  const [terminalOpen, setTerminalOpen] = useState(false);
  // The Stash button in the title bar opens the same dialog as the one under the staging lists.
  const [stashOpen, setStashOpen] = useState(false);
  // Why the last fetch failed (shown as a warning beside "origin"), null while fetching works.
  const [fetchError, setFetchError] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<{ id: string; shortId: string } | null>(null);
  // The base commit of an open interactive rebase (its screen covers the sidebar and the graph).
  const [rebasing, setRebasing] = useState<{ id: string; shortId: string } | null>(null);
  // The commit whose files are shown in the right panel (instead of the working-directory changes).
  const [selectedCommit, setSelectedCommit] = useState<{ id: string; shortId: string } | null>(null);
  // A file of the selected commit shown in the centre instead of the graph.
  const [openFile, setOpenFile] = useState<CommitFile | null>(null);
  // An uncommitted file (staged or not) shown in the centre; chosen from the Changes panel.
  const [openWorkingFile, setOpenWorkingFile] = useState<{ path: string; staged: boolean; status: ChangeKind } | null>(null);
  // The history of one file shown in the centre (from the Changes right-click menu), and the commit of it whose diff is open.
  const [history, setHistory] = useState<{
    file: string;
    /** Start at this commit (opened from a commit's file list); null = the newest commits. */
    from: { id: string; shortId: string } | null;
    entry: HistoryEntry | null;
  } | null>(null);
  const path = repo.path;
  const changeCount = useWorkingChangeCount(path, graphKey);

  // Right-click > Interactive rebase: its screen replaces the sidebar and the graph; the right panel
  // goes back to the working-directory changes.
  const startRebase = (base: { id: string; shortId: string }) => {
    setRenaming(null);
    closeCommit();
    setOpenWorkingFile(null);
    setHistory(null);
    setRebasing(base);
  };
  const cancelRebase = useCallback(() => {
    setRebasing(null);
    setSelectedCommit(null); // back to the working-directory changes
    setOpenFile(null);
  }, []);

  const selectCommit = (commit: { id: string; shortId: string }) => {
    setSelectedCommit(commit);
    setOpenFile(null);
    setOpenWorkingFile(null);
    setHistory(null);
  };
  // Right-click > Drop commit: confirm what will be lost and rewritten, then drop it.
  const dropCommit = async (commit: { id: string; shortId: string }) => {
    try {
      const info = await getDropInfo(path, commit.id);
      if (!(await confirmDialog(t.repo.dropConfirm(info), t.repo.dropTitle, true, t.repo.dropTitle))) return;
      await dropLatestCommit(path, commit.id);
      closeCommit(); // the dropped commit may be the one shown in the right panel
      reload();
    } catch (e) {
      await showError(String(e), t.repo.dropTitle);
    }
  };

  // Right-click > Reset to this commit (soft / mixed / hard): show the consequences, then reset.
  const resetCommit = async (commit: { id: string; shortId: string }, mode: ResetMode) => {
    try {
      const info = await getResetInfo(path, commit.id);
      if (!(await confirmDialog(describeReset(info, mode), t.repo.resetTitle(mode), true, t.repo.resetOk(mode)))) return;
      await resetToCommit(path, commit.id, mode);
      closeCommit(); // the selected commit may no longer be on the branch
      setOpenWorkingFile(null); // the files may have changed under an open working-tree diff
      reload();
    } catch (e) {
      await showError(String(e), t.repo.resetTitle(mode));
    }
  };

  // Right-click > Fast-forward (on a commit or a local branch): move the checked-out branch forward to the target.
  const fastForwardTo = async (target: string) => {
    try {
      await fastForward(path, target);
      setOpenWorkingFile(null); // the files may have changed under an open working-tree diff
      reload();
    } catch (e) {
      await showError(String(e), t.repo.fastForwardTitle);
    }
  };

  // Right-click > Merge on a branch: merge it into the checked-out branch after a confirmation.
  const mergeInto = async (target: string, source: string) => {
    try {
      if (!(await confirmDialog(t.repo.mergeConfirm(source), t.repo.mergeTitle, false, t.repo.mergeOk))) return;
      const outcome = await mergeBranch(path, target);
      setOpenWorkingFile(null); // the files may have changed under an open working-tree diff
      reload();
      if (outcome === "upToDate") await showInfo(t.repo.mergeUpToDate(source), t.repo.mergeTitle);
    } catch (e) {
      await showError(String(e), t.repo.mergeTitle);
    }
  };

  // Right-click on a stash in the graph: apply it (keeping it), pop it, or delete it after a confirmation.
  const stashAction = async (action: "apply" | "pop" | "drop", stash: { id: string; label: string; message: string }) => {
    try {
      if (action === "drop") {
        const ok = await confirmDialog(
          t.stashes.deleteConfirm(stash.label, stash.message),
          t.stashes.delete,
          true,
          t.common.delete,
        );
        if (!ok) return;
        await dropStash(path, stash.id);
      } else if (action === "pop") {
        await popStash(path, stash.id);
      } else {
        await applyStash(path, stash.id);
      }
      if (action !== "apply" && selectedCommit?.id === stash.id) closeCommit(); // its detail view has nothing left to show
      setOpenWorkingFile(null); // the files may have changed under an open working-tree diff
      reload();
    } catch (e) {
      await showError(String(e), t.repo.stashTitle);
    }
  };

  const closeCommit = () => {
    setSelectedCommit(null);
    setOpenFile(null);
  };

  // Redraw everything and refresh the branch label (a first commit creates the branch).
  const reload = useCallback(() => {
    setGraphKey((k) => k + 1);
    openRepo(path).then(onRepoChange).catch(() => {});
  }, [path, onRepoChange]);

  // Follow changes made outside the app (terminal, editor, other tools).
  useEffect(() => {
    watchRepo(path).catch(() => {});
    const unlisten = listen("repo-changed", reload);
    return () => {
      unlisten.then((fn) => fn());
      unwatchRepo().catch(() => {});
    };
  }, [path, reload]);

  // Ctrl+` toggles the terminal.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.code === "Backquote") {
        e.preventDefault();
        setTerminalOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="shell">
      <header className="titlebar">
        <button className="ghost" onClick={onClose}>
          {t.repo.back}
        </button>
        <strong>{repo.name}</strong>
        <span className="branch">
          {repo.detached ? t.repo.detached : ""}
          {repo.head ?? t.repo.noCommits}
        </span>
        <BranchButton path={path} onCreated={reload} />
        <SyncBar path={path} refreshKey={graphKey} onFetchError={setFetchError} />
        <button
          className="syncbtn stashbtn"
          disabled={changeCount === 0}
          onClick={() => setStashOpen(true)}
          title={changeCount === 0 ? t.repo.stashNothing : t.changes.stashAllHint}
        >
          {t.repo.stash}
        </button>
        <button
          className={"syncbtn termtoggle" + (terminalOpen ? " active" : "")}
          onClick={() => setTerminalOpen((o) => !o)}
          title={t.repo.terminalHint}
        >
          {t.repo.terminal}
        </button>
      </header>
      <div className="body">
        <div className="workarea">
          {/* Sidebar and graph stay mounted under the rebase screen, so they come back as they were. */}
          <div className={"workarea-content" + (rebasing ? " covered" : "")} inert={rebasing !== null}>
            <Sidebar
              path={path}
              refreshKey={graphKey}
              onChanged={reload}
              fetchError={fetchError}
              selectedId={selectedCommit?.id ?? null}
              onSelectCommit={selectCommit}
              onStashPopped={(id) => {
                if (selectedCommit?.id === id) closeCommit(); // its detail view has nothing left to show
                reload();
              }}
              onFastForward={fastForwardTo}
              onMerge={mergeInto}
              onStashApplied={reload}
              onStashDropped={(id) => {
                if (selectedCommit?.id === id) closeCommit();
                reload();
              }}
            />
            <div className="center">
              {/* The graph stays mounted (just hidden) while a file is open, so its scroll position survives. */}
              <div className={"center-pane" + (openFile || openWorkingFile || history ? " hidden" : "")}>
                <Graph
                  path={path}
                  refreshKey={graphKey}
                  selectedId={selectedCommit?.id ?? null}
                  keyboard={!rebasing && !openFile && !openWorkingFile && !renaming && !history}
                  onSelectCommit={selectCommit}
                  onSelectWip={() => {
                    closeCommit(); // back to the working-directory changes in the right panel
                    setRenaming(null);
                    setRebasing(null);
                  }}
                  onRenameCommit={(c) => {
                    setRebasing(null);
                    setRenaming(c);
                  }}
                  onInteractiveRebase={startRebase}
                  onDropCommit={dropCommit}
                  onResetCommit={resetCommit}
                  onFastForward={(c) => fastForwardTo(c.id)}
                  onStashAction={stashAction}
                  hasChanges={changeCount > 0}
                />
              </div>
              {history && !rebasing && (
                <div className={"center-pane" + (history.entry ? " hidden" : "")}>
                  <FileHistory
                    path={path}
                    file={history.file}
                    from={history.from}
                    refreshKey={graphKey}
                    active={history.entry === null && !openFile && !openWorkingFile}
                    openId={history.entry?.id ?? null}
                    onOpen={(entry) => setHistory({ ...history, entry })}
                    onClose={() => setHistory(null)}
                  />
                </div>
              )}
              {history?.entry && !rebasing && (
                <FileDiff
                  path={path}
                  source={{ kind: "commit", id: history.entry.id, shortId: history.entry.shortId }}
                  file={{ path: history.entry.path, oldPath: history.entry.oldPath, status: history.entry.status }}
                  backLabel={t.fileHistory.backToList}
                  backHint={t.fileHistory.backToListHint}
                  onClose={() => setHistory({ ...history, entry: null })}
                />
              )}
              {openFile && selectedCommit && !rebasing && (
                <FileDiff
                  path={path}
                  source={{ kind: "commit", id: selectedCommit.id, shortId: selectedCommit.shortId }}
                  file={openFile}
                  onClose={() => setOpenFile(null)}
                />
              )}
              {openWorkingFile && !openFile && (
                <FileDiff
                  path={path}
                  source={{ kind: openWorkingFile.staged ? "staged" : "unstaged" }}
                  file={{ path: openWorkingFile.path, status: openWorkingFile.status }}
                  refreshKey={graphKey}
                  onChanged={reload} // a staged or discarded hunk changes the staging lists and the graph
                  onClose={() => setOpenWorkingFile(null)}
                />
              )}
            </div>
          </div>
          {rebasing && (
            <InteractiveRebase
              path={path}
              base={rebasing}
              selectedId={selectedCommit?.id ?? null}
              diffOpen={openFile !== null}
              onSelectCommit={selectCommit}
              onCancel={cancelRebase}
              onApplied={() => {
                setRebasing(null);
                closeCommit(); // the reworded commits (and the ones after them) have new ids
                reload();
              }}
            />
          )}
          {/* A file of the selected commit opens over the rebase screen; Back returns to the list. */}
          {rebasing && openFile && selectedCommit && (
            <div className="rebase-diff">
              <FileDiff
                path={path}
                source={{ kind: "commit", id: selectedCommit.id, shortId: selectedCommit.shortId }}
                file={openFile}
                onClose={() => setOpenFile(null)}
              />
            </div>
          )}
        </div>
        <ResizablePanel edge="left" storageKey="changesWidth" defaultWidth={340} min={260}>
          {renaming && (
            <RenameCommit
              path={path}
              commit={renaming}
              onClose={() => setRenaming(null)}
              onRenamed={() => {
                setRenaming(null);
                closeCommit(); // renaming gives the commit (and its successors) new ids
                reload();
              }}
            />
          )}
          {!renaming && selectedCommit && (
            <CommitDetail
              path={path}
              commit={selectedCommit}
              refreshKey={graphKey}
              selectedPath={openFile?.path ?? null}
              onSelectFile={setOpenFile}
              onFileHistory={
                rebasing
                  ? undefined
                  : (file) => {
                      setOpenFile(null);
                      setOpenWorkingFile(null);
                      setHistory({ file: file.path, from: selectedCommit, entry: null });
                    }
              }
              onStashPopped={() => {
                closeCommit(); // the stash is gone; the right panel shows the restored changes
                reload();
              }}
              onClose={closeCommit}
            />
          )}
          <Changes
            path={path}
            refreshKey={graphKey}
            hidden={renaming !== null || selectedCommit !== null}
            selected={openWorkingFile && { path: openWorkingFile.path, staged: openWorkingFile.staged }}
            // A diff would open underneath the rebase screen.
            onSelectFile={(f) => {
              if (rebasing) return;
              setHistory(null); // the file diff takes over the centre
              setOpenWorkingFile(f);
            }}
            onFileHistory={(file) => {
              if (rebasing) return;
              setOpenFile(null);
              setOpenWorkingFile(null);
              setHistory({ file, from: null, entry: null });
            }}
            onCommitted={() => {
              setOpenWorkingFile(null); // what was committed no longer has a working diff
              reload();
            }}
          />
        </ResizablePanel>
      </div>
      {stashOpen && (
        <StashDialog
          path={path}
          fileCount={changeCount}
          onClose={() => setStashOpen(false)}
          onStashed={() => {
            setStashOpen(false);
            setOpenWorkingFile(null); // what was stashed no longer has a working diff
            reload();
          }}
        />
      )}
      <TerminalPanel path={path} open={terminalOpen} onClose={() => setTerminalOpen(false)} />
    </div>
  );
}
