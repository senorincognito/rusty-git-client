import { useSyncExternalStore } from "react";
import { termList } from "@/api/terminal";

/** What an agent session is doing, judged only by its output (it doesn't understand the agent). */
export type AgentStatus = "working" | "idle" | "exited";

export interface SessionState {
  path: string;
  /** The agent preset started in it, null for a plain shell. */
  agent: string | null;
  running: boolean;
  status: AgentStatus;
}

const POLL_MS = 2000;
// Output within this long ago counts as working.
const WORKING_MS = 3000;

// One poll for the whole app, shared by every component that shows a dot; it runs while at least one is mounted.
let sessions = new Map<string, SessionState>();
let signature = "";
let timer: ReturnType<typeof setInterval> | undefined;
const listeners = new Set<() => void>();

/** Asks the backend now (after starting or stopping a session) instead of waiting for the next tick. */
export async function refreshTerminalSessions() {
  try {
    const list = await termList();
    const next: SessionState[] = list.map((s) => ({
      path: s.path,
      agent: s.agent,
      running: s.running,
      status: !s.running ? "exited" : s.idleMs < WORKING_MS ? "working" : "idle",
    }));
    // The status, not the raw idle time, is compared: the components only re-render when a dot would change.
    const sig = JSON.stringify(next);
    if (sig === signature) return;
    signature = sig;
    sessions = new Map(next.map((s) => [s.path, s]));
    listeners.forEach((l) => l());
  } catch {
    /* the backend is busy or going away: keep what we have */
  }
}

const subscribe = (listener: () => void) => {
  listeners.add(listener);
  if (listeners.size === 1) {
    void refreshTerminalSessions();
    timer = setInterval(() => void refreshTerminalSessions(), POLL_MS);
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) clearInterval(timer);
  };
};

/** The terminal sessions by working-directory path (agent sessions have `agent` set). */
export const useTerminalSessions = (): Map<string, SessionState> => useSyncExternalStore(subscribe, () => sessions);
