import { useCallback, useEffect, useState } from "react";
import type { RepoInfo } from "@/api/repo";
import CredentialPrompt from "@/features/auth/CredentialPrompt";
import RepoView from "@/features/repo/RepoView";
import TabBar from "@/features/tabs/TabBar";
import Welcome from "@/features/welcome/Welcome";
import "./styles/main.scss";

function App() {
  // Open repositories, one tab each. Only the active tab's RepoView is mounted: a background tab has no watcher,
  // no auto-fetch and no window-level key handlers, at the price of losing its open diff / selection when you leave.
  const [tabs, setTabs] = useState<RepoInfo[]>([]);
  // Path of the active tab; null shows the start screen (the "+" tab, or no tabs at all).
  const [active, setActive] = useState<string | null>(null);

  const openTab = useCallback((repo: RepoInfo) => {
    setTabs((prev) => (prev.some((r) => r.path === repo.path) ? prev.map((r) => (r.path === repo.path ? repo : r)) : [...prev, repo]));
    setActive(repo.path);
  }, []);

  // Stable: RepoView's watcher effect depends on it.
  const updateTab = useCallback((repo: RepoInfo) => {
    setTabs((prev) => prev.map((r) => (r.path === repo.path ? repo : r)));
  }, []);

  const closeTab = (path: string) => {
    const i = tabs.findIndex((r) => r.path === path);
    const rest = tabs.filter((r) => r.path !== path);
    setTabs(rest);
    // Closing the active tab moves to its right neighbour (or the new last one), like a browser.
    if (active === path) setActive(rest[Math.min(i, rest.length - 1)]?.path ?? null);
  };

  // The webview's own right-click menu (Back, Reload, Inspect, ...) has no place in the app: only the menus the app
  // defines open. Components that handle a right-click call preventDefault themselves (React runs before this
  // document listener), so what is left is everything else. Text fields keep the native cut/copy/paste menu.
  useEffect(() => {
    const onContextMenu = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (e.defaultPrevented || target?.closest("input, textarea, [contenteditable=true]")) return;
      e.preventDefault();
    };
    document.addEventListener("contextmenu", onContextMenu);
    return () => document.removeEventListener("contextmenu", onContextMenu);
  }, []);

  const current = tabs.find((r) => r.path === active);

  return (
    <>
      <div className="app">
        {tabs.length > 0 && (
          <TabBar tabs={tabs} active={active} onSelect={setActive} onClose={closeTab} onNew={() => setActive(null)} />
        )}
        {current ? <RepoView key={current.path} repo={current} onRepoChange={updateTab} /> : <Welcome onOpen={openTab} />}
      </div>
      <CredentialPrompt />
    </>
  );
}

export default App;
