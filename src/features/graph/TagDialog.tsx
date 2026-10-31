import { useState } from "react";
import { createTag } from "@/api/graph";
import Modal from "@/components/Modal";
import { t } from "@/i18n";
import "./TagDialog.scss";

/** Asks for a tag name and an optional message (which makes the tag annotated), then tags the commit. */
export default function TagDialog({
  path,
  commit,
  onClose,
  onTagged,
}: {
  path: string;
  commit: { id: string; shortId: string };
  onClose: () => void;
  onTagged: () => void;
}) {
  const [name, setName] = useState("");
  const [message, setMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (name.trim() === "") return;
    setBusy(true);
    try {
      await createTag(path, commit.id, name, message.trim() === "" ? null : message);
      onTagged();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  return (
    <Modal title={t.tagDialog.title(commit.shortId)} onClose={onClose} width={460}>
      <form
        className="tag-form"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t.tagDialog.namePlaceholder}
          spellCheck={false}
          autoComplete="off"
          aria-label={t.tagDialog.nameLabel}
          disabled={busy}
        />
        <textarea
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          placeholder={t.tagDialog.messagePlaceholder}
          aria-label={t.tagDialog.messageLabel}
          rows={3}
          disabled={busy}
        />
        {error && <p className="error">{error}</p>}
        <p className="modal-hint">{t.tagDialog.hint}</p>
        <div className="modal-actions">
          <button type="button" className="secondary" onClick={onClose} disabled={busy}>
            {t.common.cancel}
          </button>
          <button type="submit" className="primary" disabled={busy || name.trim() === ""}>
            {t.tagDialog.submit}
          </button>
        </div>
      </form>
    </Modal>
  );
}
