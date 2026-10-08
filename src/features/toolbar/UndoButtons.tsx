import { useCallback, useEffect, useState } from "react";
import { showError } from "@/api/dialog";
import { getUndoState, redo, undo, type UndoState } from "@/api/undo";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { t } from "@/i18n";
import "./Toolbar.scss";

/**
 * Undo and Redo in the title bar (also Ctrl/Cmd+Z and Ctrl/Cmd+Shift+Z or Ctrl+Y). The backend keeps the journal of what
 * the app did; the buttons show what they would take back.
 */
export default function UndoButtons({
  path,
  refreshKey,
  disabled,
  onDone,
}: {
  path: string;
  refreshKey: number;
  /** Something else owns the screen (the interactive rebase): neither the buttons nor the keys act. */
  disabled: boolean;
  /** Undo or Redo changed the repository: the view has to catch up. */
  onDone: () => void;
}) {
  const [state, setState] = useState<UndoState>({ undoLabel: null, redoLabel: null });
  const [busy, setBusy] = useState(false);
  const start = useLatestRequest();

  const refresh = useCallback(async () => {
    const isCurrent = start();
    try {
      const s = await getUndoState(path);
      if (isCurrent()) setState(s);
    } catch {
      if (isCurrent()) setState({ undoLabel: null, redoLabel: null });
    }
  }, [path, start]);

  useEffect(() => {
    refresh();
  }, [refresh, refreshKey]);

  const run = useCallback(
    async (again: boolean) => {
      if (busy || disabled) return;
      setBusy(true);
      try {
        await (again ? redo(path) : undo(path));
      } catch (e) {
        await showError(String(e), again ? t.undo.redoTitle : t.undo.undoTitle);
      } finally {
        setBusy(false);
        onDone();
        refresh();
      }
    },
    [busy, disabled, path, onDone, refresh],
  );

  // Ctrl/Cmd+Z undoes, Ctrl/Cmd+Shift+Z and Ctrl+Y redo. In a text field (and the terminal) the key belongs to the field.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing || e.altKey || !(e.ctrlKey || e.metaKey)) return;
      const key = e.key.toLowerCase();
      const isUndo = key === "z" && !e.shiftKey;
      const isRedo = (key === "z" && e.shiftKey) || (key === "y" && !e.shiftKey && !e.metaKey);
      if (!isUndo && !isRedo) return;
      const target = e.target as HTMLElement | null;
      if (target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))) return;
      if (document.querySelector(".ctxmenu, .modal-backdrop")) return;
      e.preventDefault();
      run(isRedo);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [run]);

  return (
    <span className="undobtns">
      <button
        className="syncbtn"
        disabled={disabled || busy || state.undoLabel === null}
        onClick={() => run(false)}
        title={state.undoLabel === null ? t.undo.nothingToUndo : t.undo.undoHint(state.undoLabel)}
      >
        {t.undo.undo}
      </button>
      <button
        className="syncbtn"
        disabled={disabled || busy || state.redoLabel === null}
        onClick={() => run(true)}
        title={state.redoLabel === null ? t.undo.nothingToRedo : t.undo.redoHint(state.redoLabel)}
      >
        {t.undo.redo}
      </button>
    </span>
  );
}
