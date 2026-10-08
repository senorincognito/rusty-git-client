import { useEffect, useRef, useState } from "react";
import { getCommitDetail, type CommitDetail as CommitDetailData, type CommitFile } from "@/api/commit";
import { popStash } from "@/api/stash";
import ContextMenu from "@/components/ContextMenu";
import FileBadge from "@/components/FileBadge";
import { followSelection } from "@/hooks/followSelection";
import { useArrowKeys } from "@/hooks/useArrowKeys";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { useWorkingChangeCount } from "@/hooks/useWorkingChangeCount";
import { fill, t } from "@/i18n";
import "./CommitDetail.scss";

const dateFmt = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

/**
 * Right-hand panel for a selected commit: message, author and the files it changed.
 * While uncommitted changes exist, a notice at the top offers the way back to them.
 */
export default function CommitDetail({
  path,
  commit,
  refreshKey = 0,
  selectedPath,
  onSelectFile,
  onFileHistory,
  onStashPopped,
  onClose,
}: {
  path: string;
  commit: { id: string; shortId: string };
  refreshKey?: number;
  /** The file currently open in the centre view, if any. */
  selectedPath: string | null;
  onSelectFile: (file: CommitFile) => void;
  /** Right-click > File history: the commits that changed the file, starting at this commit. Without it there is no menu. */
  onFileHistory?: (file: CommitFile) => void;
  /** The stash shown here was applied and removed, so there is nothing left to show. */
  onStashPopped: () => void;
  onClose: () => void;
}) {
  const [detail, setDetail] = useState<CommitDetailData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const workingChanges = useWorkingChangeCount(path, refreshKey);
  const [popping, setPopping] = useState(false);
  const [popError, setPopError] = useState<string | null>(null);
  const startDetail = useLatestRequest();
  const [menu, setMenu] = useState<{ x: number; y: number; file: CommitFile } | null>(null);

  useEffect(() => {
    const isCurrent = startDetail();
    setDetail(null);
    setError(null);
    getCommitDetail(path, commit.id)
      .then((d) => isCurrent() && setDetail(d))
      .catch((e) => isCurrent() && setError(String(e)));
  }, [path, commit.id, startDetail]);

  useEffect(() => {
    setPopError(null);
  }, [commit.id]);

  const pop = async () => {
    setPopping(true);
    setPopError(null);
    try {
      await popStash(path, commit.id);
      onStashPopped();
    } catch (e) {
      setPopError(String(e));
      setPopping(false);
    }
  };

  // While a file's diff is open, Up/Down open the file above/below it.
  const files = detail?.files ?? [];
  useArrowKeys(selectedPath !== null, (step) => {
    const at = files.findIndex((f) => f.path === selectedPath);
    if (at < 0) return false;
    const next = files[at + step];
    if (next) onSelectFile(next);
    return true;
  });
  const list = useRef<HTMLUListElement>(null);
  useEffect(() => {
    followSelection(list.current, "li.selected");
  }, [selectedPath]);

  return (
    <aside className="commitdetail">
      {workingChanges > 0 && (
        <div className="wd-notice" role="status">
          <span>
            {t.commitDetail.workingChanges(workingChanges)}
          </span>
          <button className="secondary small" onClick={onClose}>
            {t.commitDetail.viewChanges}
          </button>
        </div>
      )}
      <header className="panel-head">
        <span>{t.commitDetail.title}</span>
        <code>{commit.shortId}</code>
        <button className="ghost" onClick={onClose} title={t.commitDetail.close}>
          ×
        </button>
      </header>

      {error && <p className="error side-msg">{error}</p>}
      {!detail && !error && <p className="muted side-msg">{t.common.loading}</p>}

      {detail && (
        <>
          <div className="cd-meta">
            <p className="cd-summary">{detail.summary}</p>
            {detail.body && <pre className="cd-bodytext">{detail.body}</pre>}
            <p className="cd-line">
              {detail.author} · {dateFmt.format(new Date(detail.time * 1000))}
            </p>
            {detail.stash && (
              <>
                <p className="cd-line">
                  {fill(t.commitDetail.stashContents, { stash: <strong>{detail.stash}</strong> })}
                </p>
              </>
            )}
            <p className="cd-line">
              {detail.stash
                ? t.commitDetail.madeOn(detail.parents[0])
                : detail.parents.length === 0
                  ? t.commitDetail.root
                  : t.commitDetail.parents(detail.parents)}
              {detail.isMerge && t.commitDetail.againstFirstParent}
            </p>
          </div>

          <section className="filelist">
            <header>
              <span>
                {t.commitDetail.changedFiles} <span className="count">{detail.totalFiles}</span>
              </span>
            </header>
            {detail.files.length === 0 && <p className="muted side-msg">{t.commitDetail.noFiles}</p>}
            <ul ref={list}>
              {detail.files.map((f) => (
                <li
                  key={f.path}
                  className={
                    "selectable" + (f.path === selectedPath ? " selected" : "") + (f.path === menu?.file.path ? " ctx" : "")
                  }
                  title={f.oldPath ? `${f.oldPath} → ${f.path}` : f.path}
                  role="button"
                  tabIndex={0}
                  onClick={() => onSelectFile(f)}
                  onContextMenu={(e) => {
                    if (!onFileHistory) return; // nothing to offer: leave the right-click alone
                    e.preventDefault();
                    setMenu({ x: e.clientX, y: e.clientY, file: f });
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      onSelectFile(f);
                    }
                  }}
                >
                  <FileBadge kind={f.status} />
                  <span className="fname">{f.path}</span>
                </li>
              ))}
            </ul>
            {detail.truncated && (
              <p className="muted side-msg">
                {t.commitDetail.truncated(detail.files.length, detail.totalFiles)}
              </p>
            )}
          </section>
        </>
      )}

      {menu && onFileHistory && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            {
              label: t.commitDetail.fileHistory,
              disabled: detail?.stash != null,
              title: detail?.stash ? t.commitDetail.fileHistoryStash : t.commitDetail.fileHistoryHint,
              onClick: () => onFileHistory(menu.file),
            },
          ]}
        />
      )}

      {/* Like the Stash button in the staging panel: the action sits at the bottom. */}
      {detail?.stash && (
        <footer className="cd-footer">
          {popError && <p className="error">{popError}</p>}
          {workingChanges > 0 && (
            <p className="muted">
              {t.commitDetail.popNeedsClean}
            </p>
          )}
          <button
            className="primary"
            disabled={popping || workingChanges > 0}
            onClick={pop}
            title={t.commitDetail.popHint}
          >
            {popping ? t.commitDetail.popping : t.commitDetail.pop}
          </button>
        </footer>
      )}
    </aside>
  );
}
