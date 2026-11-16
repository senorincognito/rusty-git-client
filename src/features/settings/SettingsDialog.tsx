import Modal from "@/components/Modal";
import { useTerminalAutocomplete } from "@/hooks/useSetting";
import { t } from "@/i18n";
import "./SettingsDialog.scss";

/** App-wide settings. Each change applies and is saved at once, so the dialog only has a Close button. */
export default function SettingsDialog({ onClose }: { onClose: () => void }) {
  const [autocomplete, setAutocomplete] = useTerminalAutocomplete();

  return (
    <Modal title={t.settings.title} onClose={onClose} width={480}>
      <section className="settings-group">
        <h3>{t.settings.terminal}</h3>
        <label className="switch">
          <input
            type="checkbox"
            role="switch"
            checked={autocomplete}
            onChange={(e) => setAutocomplete(e.target.checked)}
          />
          <span className="switch-track" />
          {t.settings.terminalAutocomplete}
        </label>
        <p className="modal-hint">{t.settings.terminalAutocompleteHint}</p>
      </section>
      <div className="modal-actions">
        <button className="primary" onClick={onClose}>
          {t.settings.close}
        </button>
      </div>
    </Modal>
  );
}
