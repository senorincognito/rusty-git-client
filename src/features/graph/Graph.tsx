import { useCallback, useEffect, useRef, useState, useMemo } from "react";
import { useArrowKeys } from "@/hooks/useArrowKeys";
import { getGraph, type Edge, type Graph as GraphData, type GraphRow } from "@/api/graph";
import type { ResetMode } from "@/api/history";
import ContextMenu from "@/components/ContextMenu";
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
  onStashAction,
  hasChanges,
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
  /** Right-click on a stash: apply, pop or delete it. */
  onStashAction: (action: "apply" | "pop" | "drop", stash: { id: string; label: string; message: string }) => void;
  /** There are uncommitted changes, so a stash can't be applied cleanly. */
  hasChanges: boolean;
}) {
  const [graph, setGraph] = useState<GraphData | null>(null);
  const [limit, setLimit] = useState(PAGE);
  const [error, setError] = useState<string | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewH, setViewH] = useState(600);
  const scroller = useRef<HTMLDivElement>(null);
  const loading = useRef(false);
  const [focusTick, setFocusTick] = useState(0);
  const [menu, setMenu] = useState<{ x: number; y: number; row: GraphRow } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);

  // Reset when switching repositories.
  useEffect(() => {
    setGraph(null);
    setLimit(PAGE);
    setMenu(null);
    scroller.current?.scrollTo({ top: 0 });
  }, [path]);

  useEffect(() => {
    let stale = false;
    loading.current = true;
    getGraph(path, limit)
      .then((g) => !stale && (setGraph(g), setError(null)))
      .catch((e) => !stale && setError(String(e)))
      .finally(() => (loading.current = false));
    return () => {
      stale = true;
    };
  }, [path, limit, refreshKey, focusTick]);

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

  // Load the next page when the user nears the end.
  useEffect(() => {
    if (graph?.hasMore && !loading.current && last >= rows.length - 50) {
      setLimit((l) => l + PAGE);
    }
  }, [graph, last, rows.length]);

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

  const laneWidth = Math.min(Math.max(graph?.maxLanes ?? 1, 1), 24) * LANE_W + 4;

  if (error) return <p className="error pad">{error}</p>;
  if (graph && rows.length === 0) return <p className="muted pad">{t.graph.noCommits}</p>;

  return (
    <div className="graph" ref={scroller} onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}>
      <div style={{ height: rows.length * ROW_H, position: "relative" }}>
        {rows.slice(first, last).map((row, i) => (
          <div
            key={row.id}
            className={
              "row" +
              (row.isWip ? " wip" : "") +
              ((row.isWip ? selectedId === null : row.id === selectedId) ? " selected" : "") +
              (row.id === menu?.row.id ? " ctx" : "")
            }
            style={{ top: (first + i) * ROW_H, height: ROW_H }}
            onClick={() => (row.isWip ? onSelectWip() : onSelectCommit({ id: row.id, shortId: row.shortId }))}
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
            {
              label: t.graph.rename,
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
