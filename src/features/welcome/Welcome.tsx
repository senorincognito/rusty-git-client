import { useCallback, useEffect, useMemo, useState } from "react";
import { pickFolder } from "@/api/dialog";
import {
  getRecentRepos,
  getRepoFolder,
  openRepo,
  removeRecentRepo,
  scanRepoFolder,
  setRepoFolder,
  type FolderRepo,
  type RepoInfo,
} from "@/api/repo";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { t } from "@/i18n";
import { matchesFilter } from "@/features/sidebar/filter";
import "./Welcome.scss";

const relativeTime = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });

/** "3 days ago", "last month": how long ago a unix timestamp (seconds) was. */
function ago(seconds: number): string {
  const diff = seconds - Date.now() / 1000;
  const units: [Intl.RelativeTimeFormatUnit, number][] = [
    ["year", 31536000],
    ["month", 2592000],
    ["day", 86400],
    ["hour", 3600],
    ["minute", 60],
  ];
  for (const [unit, size] of units) {
    if (Math.abs(diff) >= size) return relativeTime.format(Math.round(diff / size), unit);
  }
  return relativeTime.format(0, "second");
}

/**
 * Start screen, two panels side by side: the recently opened repositories on the left, and on the right every
 * repository found inside a folder the user picked (remembered between sessions) for quick switching.
 */
export default function Welcome({ onOpen }: { onOpen: (repo: RepoInfo) => void }) {
  const [recents, setRecents] = useState<RepoInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [folder, setFolder] = useState<string | null>(null);
  const [repos, setRepos] = useState<FolderRepo[] | null>(null);
  const [scanError, setScanError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const startScan = useLatestRequest();

  const refreshRecents = useCallback(
    () => getRecentRepos().then(setRecents).catch(() => setRecents([])),
    [],
  );

  const scan = useCallback(
    async (path: string) => {
      const isCurrent = startScan();
      setRepos(null);
      setScanError(null);
      try {
        const found = await scanRepoFolder(path);
        if (isCurrent()) setRepos(found);
      } catch (e) {
        if (isCurrent()) {
          setRepos([]);
          setScanError(String(e));
        }
      }
    },
    [startScan],
  );

  useEffect(() => {
    refreshRecents();
    getRepoFolder()
      .then((f) => {
        setFolder(f);
        if (f) scan(f);
      })
      .catch(() => {});
  }, [refreshRecents, scan]);

  const open = useCallback(
    async (path: string) => {
      try {
        setError(null);
        onOpen(await openRepo(path));
      } catch (e) {
        setError(String(e));
      }
    },
    [onOpen],
  );

  const browse = useCallback(async () => {
    const path = await pickFolder();
    if (path) await open(path);
  }, [open]);

  const chooseFolder = useCallback(async () => {
    const path = await pickFolder(t.welcome.folderPickerTitle);
    if (!path) return;
    try {
      await setRepoFolder(path);
      setFolder(path);
      setFilter("");
      scan(path);
    } catch (e) {
      setError(String(e));
    }
  }, [scan]);

  const forgetFolder = useCallback(async () => {
    await setRepoFolder(null).catch(() => {});
    startScan(); // drop a scan that is still running
    setFolder(null);
    setRepos(null);
    setScanError(null);
  }, [startScan]);

  const shown = useMemo(
    () => repos?.filter((r) => matchesFilter(filter, r.name, r.parent, r.head ?? "")),
    [repos, filter],
  );

  return (
    <div className="welcome">
      <header className="welcome-head">
        <h1>{t.welcome.title}</h1>
        <button className="primary" onClick={browse}>
          {t.welcome.open}
        </button>
      </header>
      {error && <p className="error welcome-error">{error}</p>}

      <div className="welcome-panels">
        <section className="welcome-panel recent-panel">
          <h2>{t.welcome.recent}</h2>
          {recents.length === 0 ? (
            <p className="muted">{t.welcome.noRecent}</p>
          ) : (
            <ul className="recents">
              {recents.map((r) => (
                <li key={r.path}>
                  <button className="recent" onClick={() => open(r.path)} title={r.path}>
                    <span className="name">{r.name}</span>
                    <span className="path">{r.path}</span>
                  </button>
                  <button
                    className="ghost"
                    title={t.welcome.removeRecent}
                    aria-label={t.welcome.removeRecent}
                    onClick={() => removeRecentRepo(r.path).then(refreshRecents)}
                  >
                    ×
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="welcome-panel folder-panel">
          <div className="folder-head">
            <h2>{t.welcome.repositories}</h2>
            {folder && (
              <span className="folder-actions">
                <button className="ghost" onClick={chooseFolder}>
                  {t.welcome.changeFolder}
                </button>
                <button className="ghost" title={t.welcome.rescanHint} aria-label={t.welcome.rescanHint} onClick={() => scan(folder)}>
                  {t.welcome.rescan}
                </button>
                <button className="ghost" title={t.welcome.clearFolderHint} onClick={forgetFolder}>
                  {t.welcome.clearFolder}
                </button>
              </span>
            )}
          </div>

          {!folder ? (
            <div className="folder-empty">
              <p className="muted">{t.welcome.folderHint}</p>
              <button className="primary" onClick={chooseFolder}>
                {t.welcome.chooseFolder}
              </button>
            </div>
          ) : (
            <>
              <p className="folder-path" title={folder}>
                {folder}
                {repos && <span className="folder-count">{t.welcome.count(repos.length)}</span>}
              </p>
              {repos && repos.length > 0 && (
                <input
                  className="folder-filter"
                  type="text"
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                  placeholder={t.welcome.filterPlaceholder}
                  aria-label={t.welcome.filterLabel}
                  spellCheck={false}
                  autoComplete="off"
                />
              )}
              {repos === null && <p className="muted">{t.welcome.scanning}</p>}
              {scanError && <p className="error">{scanError}</p>}
              {repos?.length === 0 && !scanError && <p className="muted">{t.welcome.noRepos}</p>}
              {repos && repos.length > 0 && shown?.length === 0 && <p className="muted">{t.welcome.noMatch}</p>}
              <ul className="folder-repos">
                {shown?.map((r) => (
                  <li key={r.path}>
                    <button className="repo-card" onClick={() => open(r.path)} title={r.path}>
                      <span className="rc-top">
                        <span className="name">{r.name}</span>
                        {r.parent && <span className="rc-parent">{r.parent}</span>}
                        {r.head ? (
                          <span className={"rc-branch" + (r.detached ? " detached" : "")}>
                            {r.detached ? `${t.welcome.detached} ` : ""}
                            {r.head}
                          </span>
                        ) : null}
                      </span>
                      <span className="rc-when">
                        {r.lastCommit === null ? t.welcome.noCommits : t.welcome.committed(ago(r.lastCommit))}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </>
          )}
        </section>
      </div>
    </div>
  );
}
