import { invoke } from "@tauri-apps/api/core";

export interface UndoState {
  /** What Undo would take back, e.g. "Merge feature"; null when there is nothing to undo. */
  undoLabel: string | null;
  redoLabel: string | null;
}

export const getUndoState = (path: string) => invoke<UndoState>("get_undo_state", { path });

/** Takes back the last recorded action; resolves to its label. Rejects (and changes nothing) if that isn't safe. */
export const undo = (path: string) => invoke<string>("undo_cmd", { path });

/** Repeats the action that was last taken back; resolves to its label. */
export const redo = (path: string) => invoke<string>("redo_cmd", { path });
