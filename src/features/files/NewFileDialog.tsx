import { useState } from "react";
import { createFile } from "@/api/files";
import Modal from "@/components/Modal";
import { t } from "@/i18n";
import "./NewFileDialog.scss";

/** Asks for the path of a new, empty file (folders in it are created), then creates it. */
export default function NewFileDialog({
  path,
  folder,
  onClose,
  onCreated,
}: {
  path: string;
  /** Pre-fills the path ("" = the repository root). */
  folder: string;
  onClose: () => void;
  /** The created file's path. */
  onCreated: (file: string) => void;
}) {
  const [name, setName] = useState(folder === "" ? "" : folder + "/");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Windows separators and a leading slash are what people type; the backend wants a relative `/` path.
  const file = name.trim().replace(/\\/g, "/").replace(/^\/+/, "");

  const submit = async () => {
    if (file === "" || file.endsWith("/")) return;
    setBusy(true);
    try {
      await createFile(path, file, "");
      onCreated(file);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  return (
    <Modal title={t.files.newTitle} onClose={onClose} width={460}>
      <form
        className="newfile-form"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t.files.newPlaceholder}
          spellCheck={false}
          autoComplete="off"
          aria-label={t.files.newLabel}
          disabled={busy}
        />
        {error && <p className="error">{error}</p>}
        <p className="modal-hint">{t.files.newHint}</p>
        <div className="modal-actions">
          <button type="button" className="secondary" onClick={onClose} disabled={busy}>
            {t.common.cancel}
          </button>
          <button type="submit" className="primary" disabled={busy || file === "" || file.endsWith("/")}>
            {t.files.create}
          </button>
        </div>
      </form>
    </Modal>
  );
}
