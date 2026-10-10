import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { confirmDialog } from "@/api/dialog";
import {
  discardHunk,
  getFileDiff,
  getWorkingDiff,
  stageHunk,
  unstageHunk,
  type DiffLine,
  type FileDiff as FileDiffData,
} from "@/api/diff";
import FileBadge, { type FileStatus } from "@/components/FileBadge";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { usePersistentState } from "@/hooks/usePersistentState";
import { t } from "@/i18n";
import "./FileDiff.scss";

const ROW_H = 20;
const OVERSCAN = 20;

/** Where the diff comes from: a commit, or the staged / unstaged changes of the working tree. */
export type DiffSource = { kind: "commit"; id: string; shortId: string } | { kind: "staged" } | { kind: "unstaged" };

export interface DiffFile {
  path: string;
  oldPath?: string | null;
  status: FileStatus;
}

/** What the virtual list draws: a diff line, or the heading above a hunk. */
type Row = { type: "line"; line: DiffLine } | { type: "hunk"; block: number; adds: number; dels: number };

/**
 * The centre view for one file: its content with the added and removed lines marked in place
 * (or just the changed hunks). Every run of changed lines is a hunk with a heading row; on the
 * unstaged changes of a tracked file it has "Stage hunk" and "Discard hunk" buttons, on the staged
 * changes an "Unstage hunk" button.
 * Rendered in a virtual list so long files stay fast. Working-tree diffs reload when the repo
 * changes or the window regains focus, keeping the scroll position.
 */
export default function FileDiff({
  path,
  source,
  file,
  refreshKey = 0,
  onChanged,
  onClose,
  backLabel,
  backHint,
}: {
  path: string;
  source: DiffSource;
  file: DiffFile;
  refreshKey?: number;
  /** A hunk was staged or discarded, so the staging lists and the graph need to refresh. */
  onChanged?: () => void;
  onClose: () => void;
  /** Texts of the back button when the diff was opened from somewhere other than the graph or the staging lists. */
  backLabel?: string;
  backHint?: string;
}) {
  const [full, setFull] = usePersistentState("diff.fullFile", true, (v): v is boolean => typeof v === "boolean");
  const [diff, setDiff] = useState<FileDiffData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [acting, setActing] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewH, setViewH] = useState(600);
  const scroller = useRef<HTMLDivElement>(null);
  const start = useLatestRequest();

  const commitId = source.kind === "commit" ? source.id : null;
  const staged = source.kind === "staged";
  const isWorking = source.kind !== "commit";
  const oldPath = file.oldPath ?? null;

  const fetchDiff = useCallback(
    () =>
      commitId !== null
        ? getFileDiff(path, commitId, file.path, oldPath, full)
        : getWorkingDiff(path, file.path, staged, full),
    [path, commitId, staged, file.path, oldPath, full],
  );

  // Reload without clearing what is shown, so the scroll position survives.
  const reload = useCallback(() => {
    const isCurrent = start();
    return fetchDiff()
      .then((d) => {
        if (isCurrent()) setDiff(d);
      })
      .catch(() => {});
  }, [fetchDiff, start]);

  // A different file, source or view mode: start from the top.
  useEffect(() => {
    const isCurrent = start();
    setDiff(null);
    setError(null);
    setActionError(null);
    setScrollTop(0);
    scroller.current?.scrollTo({ top: 0, left: 0 });
    fetchDiff()
      .then((d) => isCurrent() && setDiff(d))
      .catch((e) => isCurrent() && setError(String(e)));
  }, [fetchDiff, start]);

  // The working tree can change under an open diff (an edit, staging, a commit): refresh quietly.
  const firstRun = useRef(true);
  useEffect(() => {
    if (!isWorking) return;
    if (firstRun.current) firstRun.current = false;
    else reload();
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, [isWorking, refreshKey, reload]);

  // Escape closes the diff, like the back button. Leave it alone whenever Escape already means
  // something else: typing in a field (commit message, branch name, the terminal) or an open menu/dialog.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented || e.isComposing) return;
      const target = e.target as HTMLElement | null;
      if (target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))) return;
      if (document.querySelector(".ctxmenu, .modal-backdrop")) return;
      onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewH(el.clientHeight));
    ro.observe(el);
    setViewH(el.clientHeight);
    return () => ro.disconnect();
  }, []);

  // Hunk headings go above the first changed line of each hunk.
  const rows = useMemo<Row[]>(() => {
    if (!diff) return [];
    const totals = diff.blocks.map(() => ({ adds: 0, dels: 0 }));
    for (const l of diff.lines) {
      if (l.block === null) continue;
      if (l.kind === "add") totals[l.block].adds += 1;
      else if (l.kind === "del") totals[l.block].dels += 1;
    }
    const out: Row[] = [];
    let headed = -1;
    for (const line of diff.lines) {
      if (line.block !== null && line.block !== headed) {
        headed = line.block;
        out.push({ type: "hunk", block: line.block, ...totals[line.block] });
      }
      out.push({ type: "line", line });
    }
    return out;
  }, [diff]);

  // Overview of the changes for the scrollbar: runs of added or removed rows, as fractions of the whole list. Only
  // useful in the full-file view, where the changes sit between long stretches of unchanged code.
  const marks = useMemo(() => {
    const out: { kind: "add" | "del"; from: number; len: number }[] = [];
    rows.forEach((row, i) => {
      if (row.type !== "line" || (row.line.kind !== "add" && row.line.kind !== "del")) return;
      const prev = out[out.length - 1];
      if (prev && prev.kind === row.line.kind && prev.from + prev.len === i) prev.len += 1;
      else out.push({ kind: row.line.kind, from: i, len: 1 });
    });
    return out;
  }, [rows]);
  // Jumping between hunks: the heading rows' offsets. A jump leaves two rows of context above the heading, so
  // "where we are" is measured two rows below the top edge of the view.
  const hunkTops = useMemo(() => {
    const tops: number[] = [];
    rows.forEach((row, i) => row.type === "hunk" && tops.push(i * ROW_H));
    return tops;
  }, [rows]);
  const CONTEXT = 2 * ROW_H;
  const maxScroll = Math.max(0, rows.length * ROW_H - viewH);
  const here = scrollTop + CONTEXT;
  const nextTop = scrollTop < maxScroll - 1 ? hunkTops.find((top) => top > here) : undefined;
  const prevTop = scrollTop > 0 ? [...hunkTops].reverse().find((top) => top < here - 1) : undefined;
  const hunksAbove = hunkTops.filter((top) => top <= here).length;
  const jumpTo = (top: number | undefined) => {
    if (top !== undefined) scroller.current?.scrollTo({ top: Math.max(0, top - CONTEXT), behavior: "smooth" });
  };
  const showMarks = full && marks.length > 0 && rows.length * ROW_H > viewH;

  // Scroll to a specific row index when a mark is clicked.
  const scrollToMark = useCallback(
    (fromIndex: number) => {
      const targetTop = Math.max(0, fromIndex * ROW_H - CONTEXT);
      scroller.current?.scrollTo({ top: targetTop, behavior: "smooth" });
    },
    [rows.length, viewH],
  );

  // Hunks can be moved only in text diffs that are complete and not mid-conflict. Untracked files
  // (unstaged "new") have no index version to build from; stage them whole from the list.
  const hunkable = diff !== null && !diff.binary && !diff.truncated && file.status !== "conflicted";
  const canStage = hunkable && source.kind === "unstaged" && file.status !== "new";
  const canUnstage = hunkable && source.kind === "staged";

  const act = async (op: () => Promise<void>) => {
    setActing(true);
    setActionError(null);
    try {
      await op();
      onChanged?.();
    } catch (e) {
      setActionError(String(e));
    } finally {
      await reload();
      setActing(false);
    }
  };

  const stage = (block: number) => {
    if (diff) act(() => stageHunk(path, file.path, block, diff.blocks[block]));
  };

  const unstage = (block: number) => {
    if (diff) act(() => unstageHunk(path, file.path, block, diff.blocks[block]));
  };

  const discard = async (block: number, adds: number, dels: number) => {
    if (!diff) return;
    const ok = await confirmDialog(
      t.diff.discardHunkConfirm(adds, dels, file.path),
      t.diff.discardHunk,
      true,
      t.diff.discardOk,
    );
    if (ok) act(() => discardHunk(path, file.path, block, diff.blocks[block]));
  };

  const first = Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN);
  const last = Math.min(rows.length, Math.ceil((scrollTop + viewH) / ROW_H) + OVERSCAN);
  const lines = diff?.lines ?? [];
  const maxNo = lines.reduce((m, l) => Math.max(m, l.oldNo ?? 0, l.newNo ?? 0), 0);
  const gutter = `${Math.max(String(maxNo).length, 2) + 1}ch`;
  const origin = source.kind === "commit" ? source.shortId : source.kind === "staged" ? t.diff.staged : t.diff.unstaged;
  const hunkCount = diff?.blocks.length ?? 0;

  return (
    <section className="filediff">
      <header className="fd-head">
        <button
          className="ghost"
          onClick={onClose}
          title={backHint ?? (isWorking ? t.diff.closeHint : t.diff.backHint)}
        >
          {backLabel ?? (isWorking ? t.diff.back : t.diff.backToGraph)}
        </button>
        <FileBadge kind={file.status} />
        <span className="fd-path" title={file.path}>
          {file.path}
        </span>
        {file.oldPath && <span className="fd-from">{t.diff.renamedFrom(file.oldPath)}</span>}
        <code className="fd-commit">{origin}</code>
        {diff && !diff.binary && (
          <span className="fd-stats">
            <span className="add">+{diff.additions}</span> <span className="del">-{diff.deletions}</span>
            {diff.truncated && <span className="muted">{t.diff.firstLines(lines.length)}</span>}
          </span>
        )}
        {hunkTops.length > 0 && (
          <span className="fd-nav">
            <button className="ghost" disabled={prevTop === undefined} onClick={() => jumpTo(prevTop)} title={t.diff.prevChange} aria-label={t.diff.prevChange}>
              ▲
            </button>
            <button className="ghost" disabled={nextTop === undefined} onClick={() => jumpTo(nextTop)} title={t.diff.nextChange} aria-label={t.diff.nextChange}>
              ▼
            </button>
            <span className="fd-navpos">{t.diff.changePosition(hunksAbove, hunkTops.length)}</span>
          </span>
        )}
        <label className="switch" title={t.diff.fullFileHint}>
          <input type="checkbox" role="switch" checked={full} onChange={(e) => setFull(e.target.checked)} />
          <span className="switch-track" aria-hidden="true" />
          <span>{t.diff.fullFile}</span>
        </label>
      </header>
      {actionError && <p className="fd-error">{actionError}</p>}

      <div className="fd-main">
      <div
        className="fd-body"
        ref={scroller}
        onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
        style={{ "--ln": gutter } as React.CSSProperties}
      >
        {error && <p className="error pad">{error}</p>}
        {!diff && !error && <p className="muted pad">{t.common.loading}</p>}
        {diff?.binary && <p className="muted pad">{t.diff.binary}</p>}
        {diff && !diff.binary && lines.length === 0 && (
          <p className="muted pad">
            {isWorking
              ? t.diff.noWorkingChanges(source.kind === "staged" ? t.diff.staged : t.diff.unstaged)
              : t.diff.noContentChanges}
          </p>
        )}
        {rows.length > 0 && (
          <div className="fd-list" style={{ height: rows.length * ROW_H }}>
            {rows.slice(first, last).map((row, i) => {
              const top = (first + i) * ROW_H;
              if (row.type === "hunk") {
                return (
                  <div key={`hunk-${row.block}`} className="dl block" style={{ top }}>
                    <span className="bk">
                      <span className="bk-title">
                        {t.diff.hunk(row.block + 1, hunkCount)}<span className="add">+{row.adds}</span>{" "}
                        <span className="del">-{row.dels}</span>
                      </span>
                      {canUnstage && (
                        <button
                          className="bk-btn"
                          disabled={acting}
                          onClick={() => unstage(row.block)}
                          title={t.diff.unstageHunkHint}
                        >
                          {t.diff.unstageHunk}
                        </button>
                      )}
                      {canStage && (
                        <>
                          <button
                            className="bk-btn"
                            disabled={acting}
                            onClick={() => stage(row.block)}
                            title={t.diff.stageHunkHint}
                          >
                            {t.diff.stageHunk}
                          </button>
                          <button
                            className="bk-btn danger"
                            disabled={acting}
                            onClick={() => discard(row.block, row.adds, row.dels)}
                            title={t.diff.discardHunkHint}
                          >
                            {t.diff.discardHunk}
                          </button>
                        </>
                      )}
                    </span>
                  </div>
                );
              }
              const l = row.line;
              return (
                <div key={`line-${first + i}`} className={`dl ${l.kind}`} style={{ top }}>
                  {l.kind === "hunk" ? (
                    <span className="tx">{l.text}</span>
                  ) : (
                    <>
                      <span className="ln">{l.oldNo ?? ""}</span>
                      <span className="ln">{l.newNo ?? ""}</span>
                      <span className="mk">{l.kind === "add" ? "+" : l.kind === "del" ? "-" : ""}</span>
                      <span className="tx">{l.text || " "}</span>
                    </>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
      {showMarks && (
        <div
          className="fd-marks"
          style={{ height: viewH }}
          aria-hidden="true"
          onClick={(e) => {
            if (e.target !== e.currentTarget) {
              const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
              const clickY = e.clientY - rect.top;
              const clickPercent = Math.min(100, Math.max(0, (clickY / rect.height) * 100));
              // Find the mark that contains this click position
              for (const m of marks) {
                const markFromPercent = (m.from / rows.length) * 100;
                const markLenPercent = (m.len / rows.length) * 100;
                if (clickPercent >= markFromPercent && clickPercent <= markFromPercent + markLenPercent) {
                  scrollToMark(m.from);
                  return;
                }
              }
            }
          }}
        >
          {marks.map((m) => {
            const markFromPercent = (m.from / rows.length) * 100;
            const markLenPercent = (m.len / rows.length) * 100;
            return (
              <span
                key={m.from}
                className={m.kind}
                style={{ top: `${markFromPercent}%`, height: `${markLenPercent}%` }}
                title={t.diff.scrollToDiff}
              />
            );
          })}
        </div>
      )}
      </div>
    </section>
  );
}
