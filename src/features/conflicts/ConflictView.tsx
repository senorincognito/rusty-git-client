import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getConflict, resolveConflictSide, saveResolution, type ConflictFile, type Segment } from "@/api/conflicts";
import FileBadge from "@/components/FileBadge";
import { t } from "@/i18n";
import "./ConflictView.scss";

type Choice = "ours" | "theirs" | "both";

/** Lines kept at each end of a long stretch both sides agree on. */
const KEEP = 3;

const lines = (text: string) => text.replace(/\r?\n$/, "").split(/\r?\n/);

function sameSegments(a: Segment[], b: Segment[]) {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** The file with each conflict replaced by what was chosen; a conflict without a choice keeps its markers. */
function build(data: ConflictFile, choices: Record<number, Choice>, eol: string) {
  const end = (s: string) => (s === "" || s.endsWith("\n") ? s : s + eol);
  let n = 0;
  return data.segments
    .map((s) => {
      if (s.kind === "text") return s.text;
      const choice = choices[n++];
      if (choice === "ours") return s.ours;
      if (choice === "theirs") return s.theirs;
      if (choice === "both") return end(s.ours) + s.theirs;
      return `<<<<<<< ${data.oursLabel}${eol}${end(s.ours)}=======${eol}${end(s.theirs)}>>>>>>> ${data.theirsLabel}${eol}`;
    })
    .join("");
}

/** One stretch both sides agree on, shortened to its ends when long (`first` / `last`: the file starts or ends there). */
function CommonRows({ text, first, last }: { text: string; first: boolean; last: boolean }) {
  const all = lines(text);
  const head = first ? 0 : KEEP;
  const tail = last ? 0 : KEEP;
  const hidden = all.length - head - tail;
  const shown = hidden > 2 ? [all.slice(0, head), all.slice(all.length - tail)] : [all, []];
  return (
    <div className="cv-common">
      {shown[0].length > 0 && <pre>{shown[0].join("\n")}</pre>}
      {hidden > 2 && <div className="cv-gap">{t.conflicts.unchanged(hidden)}</div>}
      {shown[1].length > 0 && <pre>{shown[1].join("\n")}</pre>}
    </div>
  );
}

/**
 * The centre view for one conflicted file: the current and the incoming version side by side, a choice for every
 * conflict (current, incoming or both), and the result underneath, which can also be edited by hand.
 * "Use current" / "Use incoming" in the header resolve the whole file at once.
 */
export default function ConflictView({
  path,
  file,
  refreshKey,
  onClose,
  onResolved,
}: {
  path: string;
  file: string;
  refreshKey: number;
  onClose: () => void;
  /** The file was resolved (and staged). */
  onResolved: () => void;
}) {
  const [data, setData] = useState<ConflictFile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [choices, setChoices] = useState<Record<number, Choice>>({});
  // The result as typed by hand; null while it follows the choices.
  const [manual, setManual] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const shownFile = useRef(file);

  // A new file starts from scratch; a reload of the same file (the repository changed) keeps the choices unless
  // the file itself is different now.
  useEffect(() => {
    const fresh = shownFile.current !== file;
    shownFile.current = file;
    if (fresh) setData(null);
    let live = true;
    getConflict(path, file).then(
      (d) => {
        if (!live) return;
        setError(null);
        setData((old) => {
          if (!fresh && old && sameSegments(old.segments, d.segments)) return old;
          setChoices({});
          setManual(null);
          return d;
        });
      },
      (e) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, [path, file, refreshKey]);

  // Escape closes the view, like the diff. Leave it alone while it means something else (a field, a menu, a dialog).
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

  const total = data?.segments.filter((s) => s.kind === "conflict").length ?? 0;
  const resolved = Object.keys(choices).length;
  // A textarea reads back with plain newlines: write them back the way the file had them.
  const crlf = data?.segments.some((s) => (s.kind === "text" ? s.text : s.ours + s.theirs).includes("\r\n"));
  const eol = crlf ? "\r\n" : "\n";
  const result = useMemo(() => (data ? build(data, choices, eol) : ""), [data, choices, eol]);
  const shown = manual ?? result;
  const hasMarkers = /^<{7}( |\r?$)/m.test(shown) && /^>{7}( |\r?$)/m.test(shown);
  const lineByLine = data !== null && data.oursExists && data.theirsExists && !data.binary;
  const canSave = lineByLine && !busy && !hasMarkers && (manual !== null || resolved === total);

  const act = useCallback(
    async (op: () => Promise<void>) => {
      if (busy) return;
      setBusy(true);
      try {
        await op();
        onResolved();
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    },
    [busy, onResolved],
  );

  const choose = (index: number, choice: Choice) => {
    setChoices((c) => ({ ...c, [index]: choice }));
    setManual(null);
  };

  const side = (which: "ours" | "theirs") => {
    const exists = data && (which === "ours" ? data.oursExists : data.theirsExists);
    const text = which === "ours" ? t.conflicts.useCurrent : t.conflicts.useIncoming;
    return {
      label: exists === false ? t.conflicts.deleteFile(text) : text,
      title: which === "ours" ? t.conflicts.useCurrentFileHint : t.conflicts.useIncomingFileHint,
      run: () => act(() => resolveConflictSide(path, file, which)),
    };
  };
  const ours = side("ours");
  const theirs = side("theirs");

  let conflictIndex = -1;
  return (
    <section className="filediff conflictview">
      <header className="fd-head">
        <button className="ghost" onClick={onClose} title={t.conflicts.backHint}>
          {t.conflicts.back}
        </button>
        <FileBadge kind="conflicted" />
        <span className="fd-path" title={file}>
          {file}
        </span>
        {lineByLine && <span className="cv-progress">{t.conflicts.progress(resolved, total)}</span>}
        <span className="cv-file-actions">
          <button className="secondary" disabled={!data || busy} onClick={ours.run} title={ours.title}>
            {ours.label}
          </button>
          <button className="secondary" disabled={!data || busy} onClick={theirs.run} title={theirs.title}>
            {theirs.label}
          </button>
          <button
            className="primary"
            disabled={!canSave}
            onClick={() => act(() => saveResolution(path, file, manual === null ? result : manual.replace(/\r?\n/g, eol)))}
            title={hasMarkers ? t.conflicts.markersLeft : canSave ? t.conflicts.markResolvedHint : t.conflicts.unresolvedHint}
          >
            {t.conflicts.markResolved}
          </button>
        </span>
      </header>

      {error && <p className="error cv-error">{error}</p>}
      {!data && !error && (
        <div className="cv-loading">
          <span className="spinner" aria-hidden="true" />
        </div>
      )}

      {data && !lineByLine && (
        <p className="cv-notice">
          {data.binary
            ? t.conflicts.binary
            : !data.oursExists
              ? t.conflicts.deletedCurrent(data.oursLabel)
              : t.conflicts.deletedIncoming(data.theirsLabel)}
        </p>
      )}

      {data && lineByLine && (
        <>
          <div className="cv-titles">
            <div title={t.conflicts.currentHint}>
              <strong>{t.conflicts.current}</strong> <span>{data.oursLabel}</span>
            </div>
            <div title={t.conflicts.incomingHint}>
              <strong>{t.conflicts.incoming}</strong> <span>{data.theirsLabel}</span>
            </div>
          </div>
          <div className="cv-body">
            {total === 0 && <p className="cv-notice">{t.conflicts.noMarkers}</p>}
            {data.segments.map((s, i) => {
              if (s.kind === "text") {
                return <CommonRows key={i} text={s.text} first={i === 0} last={i === data.segments.length - 1} />;
              }
              const index = ++conflictIndex;
              const choice = choices[index];
              const takes = (which: "ours" | "theirs") => choice === which || choice === "both";
              return (
                <div key={i} className={"cv-block" + (choice ? " resolved" : "")}>
                  <div className="cv-block-head">
                    <span>{t.conflicts.blockTitle(index + 1, total)}</span>
                    <span className="cv-choices">
                      {(["ours", "theirs", "both"] as const).map((c) => (
                        <button
                          key={c}
                          className={"ghost" + (choice === c ? " on" : "")}
                          aria-pressed={choice === c}
                          onClick={() => choose(index, c)}
                        >
                          {c === "ours" ? t.conflicts.useCurrent : c === "theirs" ? t.conflicts.useIncoming : t.conflicts.useBoth}
                        </button>
                      ))}
                    </span>
                  </div>
                  <div className="cv-cols">
                    <pre className={"ours" + (takes("ours") ? " taken" : choice ? " dropped" : "")}>
                      {s.ours === "" ? <em>{t.conflicts.empty}</em> : s.ours.replace(/\r?\n$/, "")}
                    </pre>
                    <pre className={"theirs" + (takes("theirs") ? " taken" : choice ? " dropped" : "")}>
                      {s.theirs === "" ? <em>{t.conflicts.empty}</em> : s.theirs.replace(/\r?\n$/, "")}
                    </pre>
                  </div>
                </div>
              );
            })}
          </div>
          <div className="cv-result">
            <div className="cv-result-head">
              <strong>{t.conflicts.result}</strong>
              <span>{manual !== null ? t.conflicts.edited : t.conflicts.resultHint}</span>
            </div>
            <textarea
              spellCheck={false}
              value={shown}
              onChange={(e) => setManual(e.target.value)}
              aria-label={t.conflicts.result}
            />
          </div>
        </>
      )}
    </section>
  );
}
