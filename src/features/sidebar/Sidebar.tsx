import { useState } from "react";
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
  fetchError,
  selectedId,
  onSelectCommit,
  onStashPopped,
  onStashApplied,
  onStashDropped,
  onAllStashesDropped,
  onFastForward,
  onMerge,
}: {
  path: string;
  refreshKey: number;
  onChanged: () => void;
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
}) {
  // One filter for every list below: local branches, remote branches and stashes.
  const [filter, setFilter] = useState("");
  return (
    <ResizablePanel edge="right" storageKey="sidebarWidth" defaultWidth={240}>
      <nav className="sidebar">
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
        <LocalBranches path={path} refreshKey={refreshKey} onChanged={onChanged} filter={filter} onFastForward={onFastForward} onMerge={onMerge} />
        <Remotes path={path} refreshKey={refreshKey} onChanged={onChanged} fetchError={fetchError} filter={filter} onMerge={onMerge} />
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
    </ResizablePanel>
  );
}
