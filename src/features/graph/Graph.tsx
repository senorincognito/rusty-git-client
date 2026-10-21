import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import { useArrowKeys } from "@/hooks/useArrowKeys";
import { getGraph, type Edge, type Graph as GraphData, type GraphRow, type RefLabel } from "@/api/graph";
import type { ResetMode } from "@/api/history";
import ContextMenu, { type MenuItem } from "@/components/ContextMenu";
import { t } from "@/i18n";
import "./Graph.scss";

const ROW_H = 28;
const LANE_W = 16;
const NODE_R = 4.5;
const PAGE = 1000;
const OVERSCAN = 10;
const COLORS = [
  "#4ea1ff",
  "#e5646b",
  "#58c98b",
  "#e0a64a",
  "#b583f0",
  "#3fc7c7",
  "#e87fb5",
  "#9aa86b",
];

const x = (col: number) => col * LANE_W + LANE_W / 2;
const color = (i: number) => COLORS[i % COLORS.length];

/** Line from the top edge at `from` to the node centre (or node centre to bottom edge). */
function curve(x1: number, y1: number, x2: number, y2: number) {
  if (x1 === x2) return `M${x1} ${y1}L${x2} ${y2}`;
  const mid = (y1 + y2) / 2;
  return `M${x1} ${y1}C${x1} ${mid} ${x2} ${mid} ${x2} ${y2}`;
}

/** The commit HEAD points at (the tip of the checked-out branch, or the detached HEAD). */
const isCurrent = (row: GraphRow) => !row.isWip && !row.isStash && row.refs.some((r) => r.isHead);

function RowGraph({ row, width }: { row: GraphRow; width: number }) {
  const mid = ROW_H / 2;
  const cx = x(row.col);
  const line = (e: Edge, d: string, key: string) => (
    <path key={key} d={d} stroke={color(e.color)} strokeWidth={2} fill="none" strokeDasharray={e.dashed ? "3 3" : undefined} />
  );
  return (
    <svg width={width} height={ROW_H} className="lanes">
      {row.through.map((e, i) => line(e, curve(x(e.col), 0, x(e.col), ROW_H), `t${i}`))}
      {row.top.map((e, i) => line(e, curve(x(e.col), 0, cx, mid), `u${i}`))}
      {row.bottom.map((e, i) => line(e, curve(cx, mid, x(e.col), ROW_H), `b${i}`))}
      {isCurrent(row) && <circle cx={cx} cy={mid} r={NODE_R + 4} fill="none" stroke="var(--accent)" strokeWidth={2} />}
      {row.isWip ? (
        <circle cx={cx} cy={mid} r={NODE_R} fill="var(--bg)" stroke={color(row.color)} strokeWidth={2} strokeDasharray="2 2" />
      ) : row.isStash ? (
        <circle cx={cx} cy={mid} r={NODE_R} fill="var(--bg)" stroke={color(row.color)} strokeWidth={2} />
      ) : (
        <circle cx={cx} cy={mid} r={NODE_R} fill={color(row.color)} stroke="var(--bg)" strokeWidth={2} />
      )}
    </svg>
  );
}

/** The branches whose tip is this commit and that can be switched to: every local or remote branch except the checked-out one. */
const checkoutTargets = (row: GraphRow): RefLabel[] =>
  row.isStash || row.isWip ? [] : row.refs.filter((r) => (r.kind === "branch" || r.kind === "remote") && !r.isHead);

/** Double-click: a local branch if there is one, else the only remote branch; with several remotes the menu decides. */
const doubleClickTarget = (row: GraphRow): RefLabel | undefined => {
  const targets = checkoutTargets(row);
  return targets.find((r) => r.kind === "branch") ?? (targets.length === 1 ? targets[0] : undefined);
};

const dateFmt = new Intl.DateTimeFormat(undefined, {
  year: "numeric",
  month: "short",
  day: "numeric",
  hour: "2-digit",
  minute: "2-digit",
});

export default function Graph({
  path,
  refreshKey = 0,
  selectedId,
  keyboard = true,
  onSelectCommit,
  onSelectWip,
  onRenameCommit,
  onInteractiveRebase,
  onDropCommit,
  onResetCommit,
  onFastForward,
  onCheckoutCommit,
  onCherryPick,
  onRevert,
  onStashAction,
  onCheckout,
  onMerge,
  onRebaseOnto,
  hasChanges,
  detached,
}: {
  path: string;
  refreshKey?: number;
  /** The commit whose details are open, if any (none: the working-directory changes are shown). */
  selectedId: string | null;
  /** Up/Down move the selection. Off while something else covers the graph (a diff, the rebase screen). */
  keyboard?: boolean;
  onSelectCommit: (commit: { id: string; shortId: string }) => void;
  /** The uncommitted-changes row was clicked. */
  onSelectWip: () => void;
  onRenameCommit: (commit: { id: string; shortId: string }) => void;
  /** Start an interactive rebase of the commits after this one. */
  onInteractiveRebase: (commit: { id: string; shortId: string }) => void;
  /** Drop a commit of the current branch (the commits after it are re-created). */
  onDropCommit: (commit: { id: string; shortId: string }) => void;
  /** Reset the branch to a commit (soft, mixed or hard). */
  onResetCommit: (commit: { id: string; shortId: string }, mode: ResetMode) => void;
  onFastForward: (commit: { id: string; shortId: string }) => void;
  /** Detach HEAD at this commit. */
  onCheckoutCommit: (commit: { id: string; shortId: string }) => void;
  /** Copy this commit onto the checked-out branch. */
  onCherryPick: (commit: { id: string; shortId: string }) => void;
  /** Add a commit that undoes this one. */
  onRevert: (commit: { id: string; shortId: string }) => void;
  /** Right-click on a stash: apply, pop or delete it. */
  onStashAction: (action: "apply" | "pop" | "drop", stash: { id: string; label: string; message: string }) => void;
  /** Check out the branch whose tip this is (a local branch, or a remote one that becomes a local branch). */
  onCheckout: (ref: RefLabel) => void;
  /** Merge a branch (full ref name, display name) into the checked-out branch. */
  onMerge: (target: string, source: string) => void;
  /** Rebase the checked-out branch onto a branch (full ref name, display name). */
  onRebaseOnto: (target: string, source: string) => void;
  /** There are uncommitted changes, so a stash can't be applied cleanly. */
  hasChanges: boolean;
  /** HEAD is detached, so a cherry-pick or revert has no branch to land on. */
  detached: boolean;
}) {
  const [graph, setGraph] = useState<GraphData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewH, setViewH] = useState(600);
  const scroller = useRef<HTMLDivElement>(null);
  const loading = useRef(false);
  // Bumped by every reload, so a page that was requested before it is dropped when it arrives.
  const generation = useRef(0);
  // Commits loaded so far (without the uncommitted-changes row): where the next page starts, and how much a reload asks for.
  const loaded = useRef(0);
  const [focusTick, setFocusTick] = useState(0);
  const [menu, setMenu] = useState<{ x: number; y: number; row: GraphRow } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);

  // Reset when switching repositories.
  useEffect(() => {
    setGraph(null);
    loaded.current = 0;
    setMenu(null);
    scroller.current?.scrollTo({ top: 0 });
  }, [path]);

  // (Re)load everything shown so far in one request, so scrolling far down survives a refresh.
  useEffect(() => {
    const mine = ++generation.current;
    loading.current = true;
    getGraph(path, 0, Math.max(PAGE, loaded.current))
      .then((g) => {
        if (generation.current !== mine) return;
        loaded.current = g.rows.filter((r) => !r.isWip).length;
        setGraph(g);
        setError(null);
      })
      .catch((e) => generation.current === mine && setError(String(e)))
      .finally(() => {
        if (generation.current === mine) loading.current = false;
      });
  }, [path, refreshKey, focusTick]);

  // Edits made in another program change the uncommitted-changes row (only .git is watched).
  useEffect(() => {
    const onFocus = () => setFocusTick((t) => t + 1);
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewH(el.clientHeight));
    ro.observe(el);
    setViewH(el.clientHeight);
    return () => ro.disconnect();
  }, []);

  const rows = useMemo(() => graph?.rows ?? [], [graph]);
  const first = Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN);
  const last = Math.min(rows.length, Math.ceil((scrollTop + viewH) / ROW_H) + OVERSCAN);

  // Up/Down move the selection to the row above/below. The uncommitted-changes row counts as selected while no
  // commit is, so Down leaves it for the first commit.
  useArrowKeys(keyboard, (step) => {
    const current = selectedId === null ? rows.findIndex((r) => r.isWip) : rows.findIndex((r) => r.id === selectedId);
    if (current < 0) return false; // nothing selected in the graph
    const row = rows[current + step];
    if (row?.isWip) onSelectWip();
    else if (row) onSelectCommit({ id: row.id, shortId: row.shortId });
    return true;
  });

  // Keep the selected row in view when the selection moves (the list is virtualised, so scroll by arithmetic).
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const index = selectedId === null ? rows.findIndex((r) => r.isWip) : rows.findIndex((r) => r.id === selectedId);
    if (index < 0) return;
    const top = index * ROW_H;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + ROW_H > el.scrollTop + el.clientHeight) el.scrollTop = top + ROW_H - el.clientHeight;
    // Only when the selection changes, not on every reload of the rows.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId]);

  // Load the next page when the user nears the end; it is appended, nothing is fetched twice.
  useEffect(() => {
    if (!graph?.hasMore || loading.current || last < rows.length - 50) return;
    const mine = generation.current;
    loading.current = true;
    getGraph(path, loaded.current, PAGE)
      .then((page) => {
        if (generation.current !== mine) return;
        loaded.current += page.rows.length;
        setGraph((prev) =>
          prev && { rows: [...prev.rows, ...page.rows], maxLanes: Math.max(prev.maxLanes, page.maxLanes), hasMore: page.hasMore },
        );
      })
      .catch((e) => generation.current === mine && setError(String(e)))
      .finally(() => {
        if (generation.current === mine) loading.current = false;
      });
  }, [graph, last, rows.length, path]);

  // Commits on the checked-out branch can be dropped (the backend also checks the details: the commit
  // must be on the branch's own line, with no merge after it, and not a detached HEAD).
  const dropReason = !menu
    ? undefined
    : menu.row.isStash
      ? t.graph.dropStash
      : !menu.row.onHead
        ? t.graph.dropNotOnBranch
        : menu.row.parents.length === 0
          ? t.graph.dropFirst
          : undefined;

  // An interactive rebase covers the commits after this one on the current branch (the backend also
  // requires it to be on the branch's own first-parent line).
  const rebaseReason = !menu
    ? undefined
    : menu.row.isStash
      ? t.graph.rebaseStash
      : !menu.row.onHead
        ? t.graph.rebaseNotOnBranch
        : rows.find((r) => r.onHead && !r.isWip && !r.isStash)?.id === menu.row.id
          ? t.graph.rebaseNothingAfter
          : undefined;

  // "Check out" for the branches whose tip is the clicked commit: one item, or a group when there are several.
  const checkoutItems = (row: GraphRow): MenuItem[] => {
    const targets = checkoutTargets(row);
    if (targets.length === 0) return [];
    if (targets.length === 1) {
      const [only] = targets;
      return [{ label: t.graph.checkout(only.name), title: t.graph.checkoutHint(only.name), onClick: () => onCheckout(only) }];
    }
    return [
      {
        label: t.graph.checkoutGroup,
        children: targets.map((r) => ({ label: r.name, title: t.graph.checkoutHint(r.name), onClick: () => onCheckout(r) })),
      },
    ];
  };

  // "Copy branch name" for every branch label on the row (the checked-out one too): one item, or a group when there are several.
  const copyNameItems = (row: GraphRow): MenuItem[] => {
    const names = row.refs.filter((r) => r.kind === "branch" || r.kind === "remote");
    const copy = (name: string) => navigator.clipboard?.writeText(name).catch(() => {});
    if (names.length === 0) return [];
    if (names.length === 1) {
      return [{ label: t.common.copyBranchName, title: t.common.copyBranchNameHint, onClick: () => copy(names[0].name) }];
    }
    return [
      {
        label: t.common.copyBranchName,
        title: t.common.copyBranchNameHint,
        children: names.map((r) => ({ label: r.name, onClick: () => copy(r.name) })),
      },
    ];
  };

  // "Merge" / "Rebase onto" for the same branches (never the checked-out one): one item, or a group when there are several.
  const branchActionItems = (row: GraphRow): MenuItem[] => {
    const targets = checkoutTargets(row);
    const fullRef = (r: RefLabel) => (r.kind === "remote" ? `refs/remotes/${r.name}` : `refs/heads/${r.name}`);
    const make = (
      label: (name: string) => string,
      group: string,
      hint: string,
      run: (target: string, source: string) => void,
    ): MenuItem[] => {
      const leaf = (r: RefLabel, text: string): MenuItem => ({
        label: text,
        disabled: detached,
        title: detached ? t.graph.branchDetached : hint,
        onClick: () => run(fullRef(r), r.name),
      });
      if (targets.length === 0) return [];
      if (targets.length === 1) return [leaf(targets[0], label(targets[0].name))];
      return [{ label: group, disabled: detached, title: detached ? t.graph.branchDetached : hint, children: targets.map((r) => leaf(r, r.name)) }];
    };
    return [
      ...make(t.graph.mergeBranch, t.graph.mergeGroup, t.graph.mergeBranchHint, onMerge),
      ...make(t.graph.rebaseBranch, t.graph.rebaseGroup, t.graph.rebaseBranchHint, onRebaseOnto),
    ];
  };

  const laneWidth = Math.min(Math.max(graph?.maxLanes ?? 1, 1), 24) * LANE_W + 4;

  if (error) return <p className="error pad">{error}</p>;
  if (graph && rows.length === 0) return <p className="muted pad">{t.graph.noCommits}</p>;

  return (
    <div className="graph" ref={scroller} onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}>
      {!graph && (
        <div className="graph-loading" role="status">
          <span className="spinner" aria-hidden="true" />
          <strong>{t.graph.loading}</strong>
          <span className="muted">{t.graph.loadingHint}</span>
        </div>
      )}
      <div style={{ height: rows.length * ROW_H, position: "relative" }}>
        {rows.slice(first, last).map((row, i) => (
          <div
            key={row.id}
            className={
              "row" +
              (row.isWip ? " wip" : "") +
              ((row.isWip ? selectedId === null : row.id === selectedId) ? " selected" : "") +
              (row.id === menu?.row.id ? " ctx" : "") +
              (isCurrent(row) ? " current" : "") +
              // Commits that are not part of the checked-out branch's history fade back, so its history stands out.
              (!row.onHead && !row.isWip && !row.isStash ? " off" : "")
            }
            style={{ top: (first + i) * ROW_H, height: ROW_H }}
            onClick={() => (row.isWip ? onSelectWip() : onSelectCommit({ id: row.id, shortId: row.shortId }))}
            onDoubleClick={() => {
              const target = doubleClickTarget(row);
              if (target) onCheckout(target);
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              if (row.isWip) return; // nothing to do with the uncommitted changes here
              setMenu({ x: e.clientX, y: e.clientY, row });
            }}
          >
            <RowGraph row={row} width={laneWidth} />
            <div className="subject">
              {row.refs.map((r) => (
                <span key={r.kind + r.name} className={`ref ${r.kind}${r.isHead ? " head" : ""}`}>
                  {r.name}
                </span>
              ))}
              <span className="summary">{row.summary}</span>
            </div>
            <span className="author">{row.author}</span>
            <span className="date">{row.isWip ? "" : dateFmt.format(new Date(row.time * 1000))}</span>
            <span className="sha">{row.shortId}</span>
          </div>
        ))}
      </div>
      {menu && menu.row.isStash && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={closeMenu}
          items={(() => {
            const stash = {
              id: menu.row.id,
              label: menu.row.refs.find((r) => r.kind === "stash")?.name ?? menu.row.shortId,
              message: menu.row.summary,
            };
            return [
              {
                label: t.stashes.apply,
                disabled: hasChanges,
                title: hasChanges ? t.stashes.popNeedsClean : t.stashes.applyHint,
                onClick: () => onStashAction("apply", stash),
              },
              {
                label: t.stashes.pop,
                disabled: hasChanges,
                title: hasChanges ? t.stashes.popNeedsClean : t.stashes.popHint,
                onClick: () => onStashAction("pop", stash),
              },
              {
                label: t.stashes.delete,
                danger: true,
                separatorBefore: true,
                title: t.stashes.deleteHint,
                onClick: () => onStashAction("drop", stash),
              },
            ];
          })()}
        />
      )}
      {menu && !menu.row.isStash && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={closeMenu}
          items={[
            ...copyNameItems(menu.row),
            ...checkoutItems(menu.row),
            ...branchActionItems(menu.row),
            {
              label: t.graph.checkoutCommit,
              disabled: isCurrent(menu.row),
              title: t.graph.checkoutCommitHint,
              onClick: () => onCheckoutCommit({ id: menu.row.id, shortId: menu.row.shortId }),
            },
            {
              label: t.graph.cherryPick,
              disabled: detached,
              title: detached ? t.graph.cherryPickDetached : t.graph.cherryPickHint,
              onClick: () => onCherryPick({ id: menu.row.id, shortId: menu.row.shortId }),
            },
            {
              label: t.graph.revert,
              disabled: detached,
              title: detached ? t.graph.cherryPickDetached : t.graph.revertHint,
              onClick: () => onRevert({ id: menu.row.id, shortId: menu.row.shortId }),
            },
            {
              label: t.graph.copySha,
              title: t.graph.copyShaHint,
              onClick: () => {
                navigator.clipboard?.writeText(menu.row.id).catch(() => {});
              },
            },
            {
              label: t.graph.rename,
              separatorBefore: true,
              disabled: !menu.row.onHead,
              title: menu.row.onHead
                ? undefined
                : menu.row.isStash
                  ? t.graph.renameStash
                  : t.graph.renameNotOnBranch,
              onClick: () => onRenameCommit({ id: menu.row.id, shortId: menu.row.shortId }),
            },
            {
              label: t.graph.rebase,
              disabled: rebaseReason !== undefined,
              title: rebaseReason ?? t.graph.rebaseHint,
              onClick: () => onInteractiveRebase({ id: menu.row.id, shortId: menu.row.shortId }),
            },
            {
              label: t.graph.drop,
              danger: true,
              disabled: dropReason !== undefined,
              title: dropReason ?? t.graph.dropHint,
              onClick: () => onDropCommit({ id: menu.row.id, shortId: menu.row.shortId }),
            },
            {
              label: t.graph.fastForward,
              separatorBefore: true,
              disabled: menu.row.isStash || menu.row.onHead,
              title: menu.row.isStash
                ? t.graph.fastForwardStash
                : menu.row.onHead
                  ? t.graph.fastForwardContained
                  : t.graph.fastForwardHint,
              onClick: () => onFastForward({ id: menu.row.id, shortId: menu.row.shortId }),
            },
            // One group; the three reset modes open to its right.
            {
              label: t.graph.reset,
              disabled: menu.row.isStash,
              title: menu.row.isStash
                ? t.graph.resetStash
                : t.graph.resetHint,
              children: (
                [
                  ["soft", t.graph.resetSoft, t.graph.resetSoftHint],
                  ["mixed", t.graph.resetMixed, t.graph.resetMixedHint],
                  ["hard", t.graph.resetHard, t.graph.resetHardHint],
                ] as const
              ).map(([mode, label, title]) => ({
                label,
                title,
                danger: mode === "hard",
                onClick: () => onResetCommit({ id: menu.row.id, shortId: menu.row.shortId }, mode),
              })),
            },
          ]}
        />
      )}
    </div>
  );
}
