import type { Panel } from "@/api/invoke";
import { usePanelBusy } from "@/hooks/usePanelBusy";
import { t } from "@/i18n";
import "./BusyBar.scss";

/** A thin moving bar along the top edge of a panel while it loads or a change is being applied. The parent must be positioned. */
export default function BusyBar({ path, panel }: { path: string; panel: Panel }) {
  if (!usePanelBusy(path, panel)) return null;
  return <div className="busybar" role="progressbar" aria-label={t.common.working} />;
}
