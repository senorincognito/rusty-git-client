import { useEffect, useState, useSyncExternalStore } from "react";
import { activityVersion, isPanelActive, subscribeActivity, type Panel } from "@/api/invoke";

// Short work never shows the bar (no flash), and it stays up briefly after the last command: a reload starts a
// moment after the change that caused it, and the bar should not blink off in between.
const SHOW_AFTER = 200;
const HOLD = 300;

/** True while the panel has work in flight for this repository (see `api/invoke.ts`). */
export function usePanelBusy(path: string, panel: Panel): boolean {
  useSyncExternalStore(subscribeActivity, activityVersion);
  const active = isPanelActive(path, panel);
  const [shown, setShown] = useState(false);
  useEffect(() => {
    const timer = setTimeout(() => setShown(active), active ? SHOW_AFTER : HOLD);
    return () => clearTimeout(timer);
  }, [active]);
  return shown;
}
