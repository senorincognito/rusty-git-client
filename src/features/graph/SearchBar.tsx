import { useEffect, useRef, useState } from "react";
import { searchCommits, type SearchHit, type SearchResult } from "@/api/graph";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { t } from "@/i18n";
import "./SearchBar.scss";

/** Waits this long after the last keystroke, so typing a word doesn't search every prefix of it. */
const DEBOUNCE_MS = 200;

/**
 * The find bar above the graph: searches commit messages and ids, reports the matches (for highlighting) and steps
 * through them with Enter / Shift+Enter or the arrow buttons.
 */
export default function SearchBar({
  path,
  refreshKey,
  selectedId,
  focusKey,
  onHits,
  onGo,
  onClose,
}: {
  path: string;
  refreshKey: number;
  selectedId: string | null;
  /** Changes when the search is asked for again while open: the field takes the focus back. */
  focusKey: number;
  onHits: (hits: SearchHit[]) => void;
  /** Select this commit and bring it into view. */
  onGo: (hit: SearchHit) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<SearchResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const start = useLatestRequest();

  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, [focusKey]);

  // Search again when the text changes and whenever the repository does (new commits may match).
  useEffect(() => {
    const isCurrent = start();
    if (query.trim() === "") {
      setResult(null);
      setError(null);
      onHits([]);
      return;
    }
    const timer = window.setTimeout(() => {
      searchCommits(path, query)
        .then((r) => {
          if (!isCurrent()) return;
          setResult(r);
          setError(null);
          onHits(r.hits);
        })
        .catch((e) => {
          if (!isCurrent()) return;
          setResult(null);
          setError(String(e));
          onHits([]);
        });
    }, DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [query, path, refreshKey, start, onHits]);

  const hits = result?.hits ?? [];
  const at = hits.findIndex((h) => h.id === selectedId);

  // From the selected hit to the next/previous one, wrapping around; from elsewhere to the first/last.
  const step = (by: 1 | -1) => {
    if (hits.length === 0) return;
    const next = at < 0 ? (by > 0 ? 0 : hits.length - 1) : (at + by + hits.length) % hits.length;
    onGo(hits[next]);
  };

  const count =
    error ??
    (result === null
      ? ""
      : hits.length === 0
        ? t.search.noMatches
        : t.search.count(at < 0 ? null : at + 1, hits.length, result.truncated));

  return (
    <div className="graph-search" role="search">
      <input
        ref={input}
        className="gs-input"
        value={query}
        placeholder={t.search.placeholder}
        aria-label={t.search.placeholder}
        spellCheck={false}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            step(e.shiftKey ? -1 : 1);
          } else if (e.key === "Escape") {
            e.preventDefault();
            onClose();
          }
        }}
      />
      <span className={"gs-count" + (error ? " error" : "")} title={error ?? undefined}>
        {count}
      </span>
      <button className="ghost" disabled={hits.length === 0} onClick={() => step(-1)} title={t.search.previous} aria-label={t.search.previous}>
        ▲
      </button>
      <button className="ghost" disabled={hits.length === 0} onClick={() => step(1)} title={t.search.next} aria-label={t.search.next}>
        ▼
      </button>
      <button className="ghost" onClick={onClose} title={t.search.close} aria-label={t.search.close}>
        ✕
      </button>
    </div>
  );
}
