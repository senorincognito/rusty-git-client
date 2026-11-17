import { invoke } from "./invoke";

/** What a panel gets when it shows a session. */
export interface TermAttach {
  /** Identifies the session in `term-data` / `term-exit` events; a restart gets a new one. */
  id: number;
  /** Live chunks numbered up to this are already in `replay`. */
  seq: number;
  /** What the session printed so far (the last ~256 KB). */
  replay: string;
  exited: boolean;
  /** The agent preset started in it, if any. */
  agent: string | null;
}

/** One terminal session, for the status dots. */
export interface TermInfo {
  /** The working directory it belongs to. */
  path: string;
  running: boolean;
  agent: string | null;
  /** The agent's program has ended and the shell is back at its prompt. */
  finished: boolean;
  /** Milliseconds since it last printed anything. */
  idleMs: number;
}

/**
 * Shows the terminal of a working directory: starts its shell when there is none (or when `restart` asks for a fresh
 * one) and resolves to what it printed so far. Live output follows as `term-data` events until `termDetach`.
 */
export const termStart = (path: string, cols: number, rows: number, restart: boolean) =>
  invoke<TermAttach>("term_start", { path, cols, rows, restart });
/** The panel went away; the session keeps running without it. */
export const termDetach = (path: string) => invoke<void>("term_detach", { path });
export const termWrite = (id: number, data: string) => invoke<void>("term_write", { id, data });
export const termResize = (id: number, cols: number, rows: number) =>
  invoke<void>("term_resize", { id, cols, rows });
/** Ends the terminal of a working directory and whatever runs in it. */
export const termStop = (path: string) => invoke<void>("term_stop", { path });
/**
 * Types a command and Enter into the terminal of a working directory (started without a panel if there is none) and
 * records that `agent` runs there. Rejects while an earlier agent session is still alive there.
 */
export const termRun = (path: string, command: string, agent: string) =>
  invoke<void>("term_run", { path, command, agent });
export const termList = () => invoke<TermInfo[]>("term_list");
/** Which of these commands have their program on PATH (one that can't be checked counts as found). */
export const checkAgentCommands = (commands: string[]) =>
  invoke<Record<string, boolean>>("check_agent_commands", { commands });
