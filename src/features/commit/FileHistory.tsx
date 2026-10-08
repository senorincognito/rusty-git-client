import { useEffect, useState } from "react";
import { getFileHistory, type HistoryEntry } from "@/api/commit";
import FileBadge from "@/components/FileBadge";
import { t } from "@/i18n";
import "./FileHistory.scss";

const dateFmt = new Intl.DateTimeFormat(undefined, { year: "numeric", month: "short", day: "numeric" });

/**
 * The centre view listing every commit that changed one file. Clicking a commit opens that commit's diff of the file
 * (the parent keeps this list mounted underneath, so Back returns to the same scroll position).
 */
export default function FileHistory({
  path,
  file,
  refreshKey,
  active,
  openId,
  onOpen,
  onClose,
}: {
  path: string;
  file: string;
  refreshKey: number;
  /** False while a commit's diff covers this view: Escape then belongs to the diff. */
  active: boolean;
  /** The commit whose diff is open, highlighted in the list. */
  openId: string | null;
  onOpen: (entry: HistoryEntry) => void;
  onClose: () => void;
}) {
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    getFileHistory(path, file)
      .then((e) => current && (setEntries(e), setError(null)))
      .catch((e) => current && setError(String(e)));
    return () => {
      current = false;
    };
  }, [path, file, refreshKey]);

  // Escape returns to the graph, unless it already means something else (a field, a menu, a dialog).
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented || e.isComposing) return;
      const target = e.target as HTMLElement | null;
      if (target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))) return;
      if (document.querySelector(".ctxmenu, .modal-backdrop")) return;
      onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active, onClose]);

  return (
    <section className="filehistory">
      <header className="fh-head">
        <button className="ghost" onClick={onClose} title={t.fileHistory.backHint}>
          {t.fileHistory.back}
        </button>
        <span className="fh-title">{t.fileHistory.title}</span>
        <span className="fh-path" title={file}>
          {file}
        </span>
        {entries && <span className="fh-count">{t.fileHistory.count(entries.length)}</span>}
      </header>
      <div className="fh-body">
        {error && <p className="error pad">{error}</p>}
        {!entries && !error && <p className="muted pad">{t.common.loading}</p>}
        {entries?.length === 0 && <p className="muted pad">{t.fileHistory.none}</p>}
        <ul>
          {entries?.map((e) => (
            <li
              key={e.id}
              className={e.id === openId ? "selected" : ""}
              title={t.fileHistory.openHint(e.shortId)}
              role="button"
              tabIndex={0}
              onClick={() => onOpen(e)}
              onKeyDown={(ev) => {
                if (ev.target === ev.currentTarget && (ev.key === "Enter" || ev.key === " ")) {
                  ev.preventDefault();
                  onOpen(e);
                }
              }}
            >
              <FileBadge kind={e.status} />
              <code className="fh-sha">{e.shortId}</code>
              <span className="fh-summary">{e.summary}</span>
              {e.oldPath && <span className="fh-from">{t.fileHistory.renamedFrom(e.oldPath)}</span>}
              <span className="fh-author">{e.author}</span>
              <span className="fh-date">{dateFmt.format(new Date(e.time * 1000))}</span>
            </li>
          ))}
        </ul>
        {entries && entries.length >= 500 && <p className="muted pad">{t.fileHistory.capped}</p>}
      </div>
    </section>
  );
}
