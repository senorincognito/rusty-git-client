import { useState } from "react";
import type { RefLabel } from "@/api/graph";
import type { RepoInfo } from "@/api/repo";
import BusyBar from "@/components/BusyBar";
import ResizablePanel from "@/components/ResizablePanel";
import { t } from "@/i18n";
import Files from "./Files";
import LocalBranches from "./LocalBranches";
import Remotes from "./Remotes";
import Stashes from "./Stashes";
import Tags from "./Tags";
import Worktrees from "./Worktrees";
import "./Sidebar.scss";

const TABS = ["branches", "stashes", "tags", "files"] as const;
export type SidebarTab = (typeof TABS)[number];
export const isSidebarTab = (v: unknown): v is SidebarTab => TABS.includes(v as SidebarTab);

export default function Sidebar({
  path,
  tab,
  onTabChange,
  openFile,
  onOpenFile,
  onFileHistory,
  onNewFile,
  onDeleteFile,
  refreshKey,
  onChanged,
  onCheckout,
  switchingTo,
  fetchError,
  selectedId,
  onSelectCommit,
  onStashPopped,
  onStashApplied,
  onStashDropped,
  onAllStashesDropped,
  onFastForward,
  onMerge,
  onRebaseOnto,
  onDeleteTag,
  onOpenWorktree,
  onWorktreeRemoved,
}: {
  path: string;
  /** The open tab; RepoView owns it because the Files tab changes the rest of the screen. */
  tab: SidebarTab;
  onTabChange: (tab: SidebarTab) => void;
  /** The file shown in the centre by the Files tab. */
  openFile: string | null;
  onOpenFile: (file: string) => void;
  /** Show the commits that changed a file of the Files tab. */
  onFileHistory: (file: string) => void;
  /** Ask for a new file's name (RepoView shows the dialog); `folder` pre-fills it. */
  onNewFile: (folder: string) => void;
  /** Delete a file of the Files tab (RepoView asks first). */
  onDeleteFile: (file: string) => void;
  refreshKey: number;
  onChanged: () => void;
  /** Switch to a local branch, or to a remote one (which becomes a local branch). Owned by RepoView, which also runs the other entry points. */
  onCheckout: (ref: RefLabel) => Promise<void>;
  /** The branch being switched to: the whole panel is locked and shows a loading state until it is done. */
  switchingTo: string | null;
  /** Why the last fetch failed, if it did. */
  fetchError: string | null;
  /** The commit whose details are open (highlights the matching stash). */
  selectedId: string | null;
  onSelectCommit: (commit: { id: string; shortId: string }) => void;
  /** A stash was popped from the list (by its commit id). */
  onStashPopped: (id: string) => void;
  /** A stash was applied from the list and kept. */
  onStashApplied: () => void;
  /** A stash was deleted from the list (by its commit id). */
  onStashDropped: (id: string) => void;
  /** Every stash was deleted from the list (by their commit ids). */
  onAllStashesDropped: (ids: string[]) => void;
  /** Fast-forward the checked-out branch to this commit id or full ref name. */
  onFastForward: (target: string) => void;
  /** Merge the branch (full ref name, display name) into the checked-out branch. */
  onMerge: (target: string, source: string) => void;
  /** Rebase the checked-out branch onto this branch (full ref name, display name). */
  onRebaseOnto: (target: string, source: string) => void;
  /** Delete a tag by name (RepoView confirms). */
  onDeleteTag: (name: string) => void;
  /** Open a worktree as a tab. */
  onOpenWorktree: (repo: RepoInfo) => void;
  /** A worktree was removed, so its tab (if open) can close. */
  onWorktreeRemoved: (path: string) => void;
}) {
  // One filter for every list below: local branches, remote branches, stashes and tags.
  const [filter, setFilter] = useState("");
  const setTab = onTabChange;
  const tabNames: Record<SidebarTab, string> = {
    branches: t.sidebar.tabBranches,
    stashes: t.sidebar.tabStashes,
    tags: t.sidebar.tabTags,
    files: t.sidebar.tabFiles,
  };
  return (
    <ResizablePanel edge="right" storageKey="sidebarWidth" defaultWidth={240}>
      <nav className="sidebar" inert={switchingTo !== null} aria-busy={switchingTo !== null}>
        <div className="sidetabs" role="tablist" aria-label={t.sidebar.tabs}>
          {TABS.map((name) => (
            <button
              key={name}
              id={`sidetab-${name}`}
              role="tab"
              aria-selected={tab === name}
              aria-controls={`sidepane-${name}`}
              tabIndex={tab === name ? 0 : -1}
              className={tab === name ? "active" : ""}
              onClick={() => setTab(name)}
              onKeyDown={(e) => {
                const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
                if (step === 0) return;
                e.preventDefault();
                const next = TABS[(TABS.indexOf(name) + step + TABS.length) % TABS.length];
                setTab(next);
                document.getElementById(`sidetab-${next}`)?.focus();
              }}
            >
              {tabNames[name]}
            </button>
          ))}
        </div>
        <div className="sidefilter">
          <input
            type="text"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape" && filter !== "") {
                e.preventDefault();
                setFilter("");
              }
            }}
            placeholder={t.sidebar.filterPlaceholder}
            aria-label={t.sidebar.filterLabel}
            spellCheck={false}
            autoComplete="off"
          />
          {filter !== "" && (
            <button className="sidefilter-clear" title={t.sidebar.clearFilterHint} aria-label={t.sidebar.clearFilter} onClick={() => setFilter("")}>
              ×
            </button>
          )}
        </div>
        {/* Every tab stays mounted (just hidden), so the lists keep their data and scroll position. */}
        <div className="sidepane" id="sidepane-branches" role="tabpanel" aria-labelledby="sidetab-branches" hidden={tab !== "branches"}>
          <LocalBranches path={path} refreshKey={refreshKey} onChanged={onChanged} onCheckout={onCheckout} filter={filter} onFastForward={onFastForward} onMerge={onMerge} onRebaseOnto={onRebaseOnto} />
          <Remotes path={path} refreshKey={refreshKey} onChanged={onChanged} onCheckout={onCheckout} fetchError={fetchError} filter={filter} onMerge={onMerge} />
          <Worktrees path={path} refreshKey={refreshKey} filter={filter} onChanged={onChanged} onOpen={onOpenWorktree} onRemoved={onWorktreeRemoved} />
        </div>
        <div className="sidepane" id="sidepane-stashes" role="tabpanel" aria-labelledby="sidetab-stashes" hidden={tab !== "stashes"}>
          <Stashes
            path={path}
            refreshKey={refreshKey}
            selectedId={selectedId}
            onSelect={onSelectCommit}
            filter={filter}
            onPopped={onStashPopped}
            onApplied={onStashApplied}
            onDropped={onStashDropped}
            onAllDropped={onAllStashesDropped}
          />
        </div>
        <div className="sidepane" id="sidepane-tags" role="tabpanel" aria-labelledby="sidetab-tags" hidden={tab !== "tags"}>
          <Tags path={path} refreshKey={refreshKey} selectedId={selectedId} onSelect={onSelectCommit} onDelete={onDeleteTag} filter={filter} />
        </div>
        <div className="sidepane" id="sidepane-files" role="tabpanel" aria-labelledby="sidetab-files" hidden={tab !== "files"}>
          <Files path={path} refreshKey={refreshKey} selected={openFile} onSelect={onOpenFile} onHistory={onFileHistory} onNewFile={onNewFile} onDelete={onDeleteFile} filter={filter} />
        </div>
      </nav>
      <BusyBar path={path} panel="sidebar" />
      {switchingTo !== null && (
        <div className="sidebar-lock" role="status">
          <span className="spinner" aria-hidden="true" />
          <strong>{t.sidebar.switching(switchingTo)}</strong>
          <span className="muted">{t.sidebar.switchingHint}</span>
        </div>
      )}
    </ResizablePanel>
  );
}
