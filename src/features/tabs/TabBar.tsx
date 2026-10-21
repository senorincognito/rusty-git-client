import type { RepoInfo } from "@/api/repo";
import { t } from "@/i18n";
import "./TabBar.scss";

/** One tab per open repository, closable like browser tabs; "+" shows the start screen to open another. */
export default function TabBar({
  tabs,
  active,
  onSelect,
  onClose,
  onNew,
}: {
  tabs: RepoInfo[];
  /** Path of the active tab; null while the start screen is shown. */
  active: string | null;
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
  onNew: () => void;
}) {
  return (
    <div className="tabbar" role="tablist">
      {tabs.map((tab) => (
        <div
          key={tab.path}
          className={"tab" + (tab.path === active ? " active" : "")}
          role="tab"
          aria-selected={tab.path === active}
          title={tab.path}
          onClick={() => onSelect(tab.path)}
          // Middle-click closes, like a browser.
          onAuxClick={(e) => {
            if (e.button === 1) {
              e.preventDefault();
              onClose(tab.path);
            }
          }}
          onMouseDown={(e) => e.button === 1 && e.preventDefault()} // no autoscroll cursor
        >
          <span className="tab-name">{tab.name}</span>
          <button
            className="tab-close"
            title={t.tabs.close}
            aria-label={t.tabs.closeTab(tab.name)}
            onClick={(e) => {
              e.stopPropagation();
              onClose(tab.path);
            }}
          >
            ×
          </button>
        </div>
      ))}
      <button
        className={"tab-new" + (active === null ? " active" : "")}
        title={t.tabs.newTab}
        aria-label={t.tabs.newTab}
        onClick={onNew}
      >
        +
      </button>
    </div>
  );
}
