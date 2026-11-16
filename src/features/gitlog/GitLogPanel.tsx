import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { clearGitLog, getGitLog, type GitLogEntry } from "@/api/gitlog";
import Splitter from "@/components/Splitter";
import { usePersistentState } from "@/hooks/usePersistentState";
import { t } from "@/i18n";
import "./GitLogPanel.scss";

const MAX_ENTRIES = 500; // as in the backend
const MIN_H = 100;
const DEFAULT_H = 200;

const isHeight = (v: unknown): v is number => typeof v === "number" && v >= MIN_H;

/** Adds `entry`, or replaces the one with the same id (a call that has ended); keeps the newest MAX_ENTRIES. */
function upsert(list: GitLogEntry[], entry: GitLogEntry): GitLogEntry[] {
  let i = list.length - 1; // the entry that ends is nearly always among the last
  while (i >= 0 && list[i].id !== entry.id) i--;
  if (i >= 0) {
    const next = list.slice();
    next[i] = entry;
    return next;
  }
  return [...list, entry].slice(-MAX_ENTRIES);
}

const quote = (arg: string) => (/[\s"']/.test(arg) ? JSON.stringify(arg) : arg);
const command = (e: GitLogEntry) => ["git", ...e.args.map(quote)].join(" ");
const folderName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;
const clock = (ms: number) => new Date(ms).toLocaleTimeString([], { hour12: false });
const duration = (ms: number) => (ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`);

/**
 * Bottom panel listing the calls to the system git the app has made, newest at the bottom. The list lives in the
 * backend (it keeps recording while this is closed); the panel loads it on mount and follows `git-log` events.
 */
export default function GitLogPanel({ onClose }: { onClose: () => void }) {
  const [entries, setEntries] = useState<GitLogEntry[]>([]);
  const [filter, setFilter] = useState("");
  const [open, setOpen] = useState<Set<number>>(new Set()); // ids whose output is shown
  const [height, setHeight] = usePersistentState("gitLog.height", DEFAULT_H, isHeight);
  const [liveHeight, setLiveHeight] = useState<number | null>(null); // while dragging; saved on release
  const dragFrom = useRef(height);
  const list = useRef<HTMLUListElement>(null);
  const atEnd = useRef(true); // follow new entries unless the user scrolled up

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    listen<GitLogEntry>("git-log", (e) => setEntries((prev) => upsert(prev, e.payload))).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    // Events that arrive while this loads are merged by id, so nothing is shown twice.
    getGitLog()
      .then((loaded) => !disposed && setEntries((prev) => loaded.reduce(upsert, prev)))
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useLayoutEffect(() => {
    const el = list.current;
    if (el && atEnd.current) el.scrollTop = el.scrollHeight;
  }, [entries, filter, open]);

  const shown = useMemo(() => {
    const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
    return words.length ? entries.filter((e) => words.every((w) => command(e).toLowerCase().includes(w))) : entries;
  }, [entries, filter]);

  const toggle = (id: number) =>
    setOpen((prev) => {
      const next = new Set(prev);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  const clear = () => {
    clearGitLog().catch(() => {});
    setEntries([]);
    setOpen(new Set());
  };

  const clamp = (h: number) => Math.min(Math.max(h, MIN_H), window.innerHeight * 0.7);
  const shownHeight = liveHeight ?? height;

  return (
    <section className="gitlog" style={{ height: shownHeight }}>
      <Splitter
        onStart={() => (dragFrom.current = shownHeight)}
        onMove={(dy) => setLiveHeight(clamp(dragFrom.current - dy))} // dragging up grows the panel
        onNudge={(dy) => setLiveHeight(clamp(dragFrom.current - dy))}
        onEnd={() => {
          if (liveHeight !== null) setHeight(liveHeight);
          setLiveHeight(null);
        }}
        onReset={() => setHeight(DEFAULT_H)}
      />
      <header className="gitlog-head">
        <span title={t.gitLog.hint}>{t.gitLog.title}</span>
        <input
          className="gitlog-filter"
          type="search"
          value={filter}
          placeholder={t.gitLog.filterPlaceholder}
          aria-label={t.gitLog.filterPlaceholder}
          onChange={(e) => setFilter(e.target.value)}
        />
        <button className="ghost" onClick={clear} disabled={entries.length === 0} title={t.gitLog.clearHint}>
          {t.gitLog.clear}
        </button>
        <button className="ghost" onClick={onClose} title={t.gitLog.hide}>
          ×
        </button>
      </header>
      {shown.length === 0 ? (
        <p className="gitlog-empty">{entries.length === 0 ? t.gitLog.empty : t.gitLog.noMatch}</p>
      ) : (
        <ul
          className="gitlog-list"
          ref={list}
          onScroll={(e) => {
            const el = e.currentTarget;
            atEnd.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
          }}
        >
          {shown.map((e) => {
            const expanded = open.has(e.id) && e.output !== "";
            return (
              <li key={e.id} className={e.running ? "running" : e.ok ? undefined : "failed"}>
                <button className="gitlog-line" onClick={() => toggle(e.id)} aria-expanded={expanded} disabled={!e.output}>
                  <span className="gitlog-time">{clock(e.time)}</span>
                  <code className="gitlog-cmd" title={command(e)}>
                    {command(e)}
                  </code>
                  <span className="gitlog-repo" title={e.path}>
                    {folderName(e.path)}
                  </span>
                  <span className="gitlog-status">
                    {e.running ? t.gitLog.running : `${e.ok ? "✓" : "✗"} ${duration(e.durationMs)}`}
                  </span>
                </button>
                {expanded && <pre className="gitlog-output">{e.output}</pre>}
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
