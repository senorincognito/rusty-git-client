import { useEffect, useRef, useState } from "react";
import { getRenameInfo, renameCommitMessage, type RenameInfo } from "@/api/history";
import { t } from "@/i18n";
import "./RenameCommit.scss";

/** Right-hand panel for editing a commit's message. "Update" rewrites the commit. */
export default function RenameCommit({
  path,
  commit,
  onClose,
  onRenamed,
}: {
  path: string;
  commit: { id: string; shortId: string };
  onClose: () => void;
  onRenamed: () => void;
}) {
  const [info, setInfo] = useState<RenameInfo | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const box = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    let stale = false;
    setInfo(null);
    setError(null);
    getRenameInfo(path, commit.id)
      .then((i) => {
        if (stale) return;
        setInfo(i);
        setMessage(i.message);
      })
      .catch((e) => !stale && setError(String(e)));
    return () => {
      stale = true;
    };
  }, [path, commit.id]);

  useEffect(() => {
    if (info) {
      box.current?.focus();
      box.current?.select();
    }
  }, [info]);

  const changed = info !== null && message.trim() !== info.message.trim();
  const canUpdate = !busy && changed && message.trim().length > 0;

  const update = async () => {
    setBusy(true);
    try {
      await renameCommitMessage(path, commit.id, message);
      onRenamed();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <aside
      className="renamebox"
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
        else if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && canUpdate) update();
      }}
    >
      <header className="panel-head">
        <span>{t.rename.title}</span>
        <code>{commit.shortId}</code>
      </header>
      <div className="renamebody">
        <label htmlFor="rename-message">{t.rename.messageLabel}</label>
        <textarea
          spellCheck={false}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          id="rename-message"
          ref={box}
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          disabled={!info || busy}
          placeholder={info || error ? undefined : t.common.loading}
        />
        {info?.pushed && (
          <p className="warn">
            {t.rename.pushed}
          </p>
        )}
        {info && info.laterCommits > 0 && (
          <p className="note">
            {t.rename.later(info.laterCommits)}
          </p>
        )}
        {error && <p className="error">{error}</p>}
      </div>
      <footer className="renamefoot">
        <button className="secondary" onClick={onClose} disabled={busy}>
          {t.common.cancel}
        </button>
        <button className="primary" onClick={update} disabled={!canUpdate}>
          {t.rename.update}
        </button>
      </footer>
    </aside>
  );
}
