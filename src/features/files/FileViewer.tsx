import { useCallback, useEffect, useMemo, useState } from "react";
import { showError } from "@/api/dialog";
import { getFileContent, getLineBlame, saveFile, type FileContent, type LineBlame } from "@/api/files";
import ContextMenu from "@/components/ContextMenu";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { t } from "@/i18n";
import { fileReference } from "./reference";
import "./FileViewer.scss";

const sizeText = (bytes: number) =>
  bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;

const ROW_H = 20; // keep in step with the line-height in FileViewer.scss
const TAB = 4;

/** Columns a line takes in the monospace view (a tab advances to the next multiple of TAB). */
function columns(line: string): number {
  let col = 0;
  for (const ch of line) col = ch === "\t" ? col + TAB - (col % TAB) : col + 1;
  return col;
}

const isMac = /Mac/.test(navigator.userAgent);

/**
 * The centre view of the Files tab: the working-directory content of one file with line numbers. *Edit* turns it into
 * a plain text area (Save writes the file, keeping its line endings); *Delete* is handed to the parent, which asks first.
 */
export default function FileViewer({
  path,
  file,
  refreshKey,
  onDirtyChange,
  onSaved,
  onDelete,
  onSendToTerminal,
}: {
  path: string;
  file: string | null;
  refreshKey: number;
  /** Whether there are unsaved edits, so the parent can ask before another file replaces this one. */
  onDirtyChange: (dirty: boolean) => void;
  /** The file was written: everything that shows the working directory should reload. */
  onSaved: () => void;
  onDelete: (file: string) => void;
  /** Type a file or line reference into the terminal (RepoView opens it). */
  onSendToTerminal: (text: string) => void;
}) {
  const [data, setData] = useState<{ file: string; content: FileContent } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [focusTick, setFocusTick] = useState(0);
  // The text being edited (LF line endings), or null while just viewing.
  const [draft, setDraft] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  // The lines picked while viewing (click, Shift+click extends; `focus` is the last one clicked), 1-based.
  const [picked, setPicked] = useState<{ anchor: number; focus: number } | null>(null);
  // The lines of the text area's selection, or just the cursor's line.
  const [caret, setCaret] = useState({ from: 1, to: 1 });
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const [scroll, setScroll] = useState({ top: 0, left: 0 });
  const [blame, setBlame] = useState<{ key: string; value: LineBlame | null } | null>(null);
  const start = useLatestRequest();
  const startBlame = useLatestRequest();

  // Edits made in another program show up when the window comes back.
  useEffect(() => {
    const onFocus = () => setFocusTick((n) => n + 1);
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  useEffect(() => {
    setDraft(null); // the parent only changes the file after the edits were given up
    setPicked(null);
    if (file === null) {
      setData(null);
      setError(null);
    }
  }, [file]);

  useEffect(() => {
    if (file === null) return;
    const isCurrent = start();
    getFileContent(path, file).then(
      (content) => {
        if (!isCurrent()) return;
        setData({ file, content });
        setError(null);
      },
      (e) => {
        if (!isCurrent()) return;
        setData(null);
        setError(String(e));
      },
    );
  }, [path, file, refreshKey, focusTick, start]);

  const content = data?.file === file ? data.content : null;
  const text = content?.content ?? null;
  const crlf = text?.includes("\r\n") ?? false;
  const lines = useMemo(() => {
    if (text === null) return 0;
    const n = text.split("\n").length;
    return text.endsWith("\n") ? n - 1 : n;
  }, [text]);
  const gutter = useMemo(() => Array.from({ length: lines }, (_, i) => i + 1).join("\n"), [lines]);

  const editing = draft !== null;
  const dirty = editing && text !== null && draft !== text.replace(/\r\n/g, "\n");
  useEffect(() => {
    onDirtyChange(dirty);
  }, [dirty, onDirtyChange]);

  // While there are edits the line numbers no longer match what git knows, so the hint waits for the save.
  const hintLine = editing ? (dirty ? null : caret.to) : (picked?.focus ?? null);
  // What a reference points at: the selected lines, if any. Unsaved edits have no line numbers git knows.
  const range = editing ? caret : picked ? { from: Math.min(picked.anchor, picked.focus), to: Math.max(picked.anchor, picked.focus) } : null;
  const reference = file === null ? "" : fileReference(file, range);
  const referenceBlocked = dirty;
  const sendReference = () => onSendToTerminal(reference);
  const copyReference = () => navigator.clipboard?.writeText(reference).catch(() => {});
  const lineAt = (e: React.MouseEvent<HTMLElement>) =>
    Math.floor((e.clientY - e.currentTarget.getBoundingClientRect().top) / ROW_H) + 1;
  const hintText = editing ? draft : text;
  useEffect(() => {
    if (file === null || hintLine === null) return;
    const key = `${file}:${hintLine}`;
    const isCurrent = startBlame();
    // Cursor keys move fast: only ask once it rests.
    const timer = window.setTimeout(() => {
      getLineBlame(path, file, hintLine).then(
        (value) => isCurrent() && setBlame({ key, value }),
        () => isCurrent() && setBlame(null),
      );
    }, 120);
    return () => window.clearTimeout(timer);
  }, [path, file, hintLine, refreshKey, startBlame]);
  const hint = (() => {
    if (file === null || hintLine === null || hintText === null || blame?.key !== `${file}:${hintLine}`) return null;
    const lineText = hintText.split("\n")[hintLine - 1];
    if (lineText === undefined) return null;
    return {
      row: hintLine - 1,
      cols: columns(lineText.replace(/\r$/, "")),
      text: blame.value ? t.files.blame(blame.value) : t.files.blameUncommitted,
    };
  })();

  const save = async () => {
    if (file === null || draft === null || saving) return;
    setSaving(true);
    try {
      // The text area reads back LF; a file that used CRLF keeps it.
      await saveFile(path, file, crlf ? draft.replace(/\r?\n/g, "\r\n") : draft);
      setDraft(null);
      onSaved();
    } catch (e) {
      await showError(String(e), t.files.saveTitle);
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="fileviewer">
      {file === null ? (
        <p className="muted pad">{t.files.pick}</p>
      ) : (
        <>
          <header className="fv-head">
            <span className="fv-path" title={file}>
              {file}
            </span>
            {dirty && <span className="fv-meta">{t.files.unsaved}</span>}
            {content && !dirty && <span className="fv-meta">{sizeText(content.size)}</span>}
            <span className="fv-actions">
              <button
                className="secondary"
                disabled={referenceBlocked}
                onClick={sendReference}
                title={referenceBlocked ? t.files.referenceDirty : t.files.sendHint(reference)}
              >
                {t.files.sendToTerminal}
              </button>
              <button
                className="secondary"
                disabled={referenceBlocked}
                onClick={copyReference}
                title={referenceBlocked ? t.files.referenceDirty : t.files.copyHint(reference)}
              >
                {t.files.copyReference}
              </button>
              {editing ? (
                <>
                  <button className="primary" disabled={saving || !dirty} onClick={save} title={t.files.saveHint}>
                    {t.files.save}
                  </button>
                  <button className="secondary" disabled={saving} onClick={() => setDraft(null)} title={t.files.cancelEditHint}>
                    {t.files.cancelEdit}
                  </button>
                </>
              ) : (
                <button
                  className="secondary"
                  disabled={content?.kind !== "text"}
                  onClick={() => {
                    if (text === null) return;
                    setDraft(text.replace(/\r\n/g, "\n"));
                    setCaret({ from: 1, to: 1 });
                    setScroll({ top: 0, left: 0 });
                  }}
                  title={t.files.editHint}
                >
                  {t.files.edit}
                </button>
              )}
              <button className="secondary" disabled={saving} onClick={() => onDelete(file)} title={t.files.deleteHint}>
                {t.files.delete}
              </button>
            </span>
          </header>
          <div className="fv-body">
            {error && <p className="error pad">{error}</p>}
            {!content && !error && <p className="muted pad">{t.common.loading}</p>}
            {content?.kind === "binary" && <p className="muted pad">{t.files.binary}</p>}
            {content?.kind === "tooLarge" && <p className="muted pad">{t.files.tooLarge(sizeText(content.size))}</p>}
            {editing && (
              <div className="fv-editwrap">
                <textarea
                  className="fv-editor"
                  value={draft}
                  autoFocus
                  spellCheck={false}
                  aria-label={t.files.editing(file)}
                  onChange={(e) => setDraft(e.target.value)}
                  onScroll={(e) => setScroll({ top: e.currentTarget.scrollTop, left: e.currentTarget.scrollLeft })}
                  // The selected lines (or the cursor's): count the newlines before each end. A selection that ends
                  // right after a newline doesn't include the next line.
                  onSelect={(e) => {
                    const el = e.currentTarget;
                    const lineOf = (i: number) => el.value.slice(0, i).split("\n").length;
                    const from = lineOf(el.selectionStart);
                    let to = lineOf(el.selectionEnd);
                    if (to > from && el.value[el.selectionEnd - 1] === "\n") to -= 1;
                    setCaret((c) => (c.from === from && c.to === to ? c : { from, to }));
                  }}
                  onKeyDown={(e) => {
                    if (e.key.toLowerCase() === "s" && (isMac ? e.metaKey : e.ctrlKey) && !e.altKey) {
                      e.preventDefault();
                      save();
                    }
                  }}
                />
                {hint && (
                  <span
                    className="fv-hint"
                    style={{ top: hint.row * ROW_H - scroll.top, left: `calc(16px + ${hint.cols + 4}ch - ${scroll.left}px)` }}
                  >
                    {hint.text}
                  </span>
                )}
              </div>
            )}
            {!editing && text !== null && text === "" && <p className="muted pad">{t.files.empty}</p>}
            {!editing && text !== null && text !== "" && (
              <div
                className="fv-code"
                // Click a line (or its number) to pick it, Shift+click to extend to another line; clicking the only
                // picked line again lets go. Right-click picks the line under it unless it is already in the range.
                onClick={(e) => {
                  const line = lineAt(e);
                  if (line < 1 || line > lines) return;
                  setPicked((p) =>
                    e.shiftKey && p
                      ? { anchor: p.anchor, focus: line }
                      : p && p.anchor === line && p.focus === line
                        ? null
                        : { anchor: line, focus: line },
                  );
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  const line = lineAt(e);
                  if (line < 1 || line > lines) return;
                  if (!range || line < range.from || line > range.to) setPicked({ anchor: line, focus: line });
                  setMenu({ x: e.clientX, y: e.clientY });
                }}
              >
                <pre className="fv-gutter" aria-hidden="true">
                  {gutter}
                </pre>
                <div className="fv-textcol">
                  <pre className="fv-text">{text}</pre>
                  {hint && (
                    <>
                      <span
                        className="fv-band"
                        style={{ top: ((range?.from ?? hint.row + 1) - 1) * ROW_H, height: ((range?.to ?? hint.row + 1) - (range?.from ?? hint.row + 1) + 1) * ROW_H }}
                      />
                      <span className="fv-hint" style={{ top: hint.row * ROW_H, left: `calc(16px + ${hint.cols + 4}ch)` }}>
                        {hint.text}
                      </span>
                    </>
                  )}
                </div>
              </div>
            )}
          </div>
        </>
      )}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={closeMenu}
          items={[
            { label: t.files.sendToTerminal, title: t.files.sendHint(reference), onClick: sendReference },
            { label: t.files.copyReference, title: t.files.copyHint(reference), onClick: copyReference },
          ]}
        />
      )}
    </section>
  );
}
