import { useCallback, useSyncExternalStore } from "react";

// One value per key for the whole app, so a switch in the Settings dialog reaches a component that is already mounted
// (usePersistentState keeps a private copy per component, which is right for UI state but not for settings).
const cache = new Map<string, unknown>();
const listeners = new Set<() => void>();

const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};

function read<T>(key: string, initial: T, valid: (value: unknown) => value is T): T {
  if (cache.has(key)) return cache.get(key) as T;
  let value = initial;
  try {
    const raw = localStorage.getItem(key);
    if (raw !== null) {
      const parsed: unknown = JSON.parse(raw);
      if (valid(parsed)) value = parsed;
    }
  } catch {
    /* unreadable: use the default */
  }
  cache.set(key, value);
  return value;
}

/** A setting persisted in localStorage and shared by every component that uses the same key. */
export function useSetting<T>(key: string, initial: T, valid: (value: unknown) => value is T): [T, (value: T) => void] {
  const value = useSyncExternalStore(subscribe, () => read(key, initial, valid));

  const set = useCallback(
    (next: T) => {
      cache.set(key, next);
      try {
        localStorage.setItem(key, JSON.stringify(next));
      } catch {
        /* storage unavailable: the setting lasts until the app closes */
      }
      listeners.forEach((l) => l());
    },
    [key],
  );

  return [value, set];
}

const isBoolean = (v: unknown): v is boolean => typeof v === "boolean";

/** Whether the terminal suggests branch and tag names while a git command is typed. */
export const useTerminalAutocomplete = () => useSetting("terminal.autocomplete", true, isBoolean);
