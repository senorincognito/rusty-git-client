import { useState } from "react";
import type { RefLabel } from "@/api/graph";
import ResizablePanel from "@/components/ResizablePanel";
import { t } from "@/i18n";
import LocalBranches from "./LocalBranches";
import Remotes from "./Remotes";
import Stashes from "./Stashes";
import "./Sidebar.scss";

export default function Sidebar({
  path,
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
}: {
  path: string;
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
}) {
  // One filter for every list below: local branches, remote branches and stashes.
  const [filter, setFilter] = useState("");
  return (
    <ResizablePanel edge="right" storageKey="sidebarWidth" defaultWidth={240}>
      <nav className="sidebar" inert={switchingTo !== null} aria-busy={switchingTo !== null}>
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
        <LocalBranches path={path} refreshKey={refreshKey} onChanged={onChanged} onCheckout={onCheckout} filter={filter} onFastForward={onFastForward} onMerge={onMerge} onRebaseOnto={onRebaseOnto} />
        <Remotes path={path} refreshKey={refreshKey} onChanged={onChanged} onCheckout={onCheckout} fetchError={fetchError} filter={filter} onMerge={onMerge} />
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
      </nav>
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
