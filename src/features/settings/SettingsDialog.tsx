import Modal from "@/components/Modal";
import { useAgentPresets, useTerminalAutocomplete, type AgentPreset } from "@/hooks/useSetting";
import { t } from "@/i18n";
import "./SettingsDialog.scss";

/** App-wide settings. Each change applies and is saved at once, so the dialog only has a Close button. */
export default function SettingsDialog({ onClose }: { onClose: () => void }) {
  const [autocomplete, setAutocomplete] = useTerminalAutocomplete();
  const [agents, setAgents] = useAgentPresets();

  const change = (i: number, patch: Partial<AgentPreset>) => setAgents(agents.map((a, j) => (j === i ? { ...a, ...patch } : a)));
  const move = (i: number, step: -1 | 1) => {
    const next = [...agents];
    [next[i], next[i + step]] = [next[i + step], next[i]];
    setAgents(next);
  };

  return (
    <Modal title={t.settings.title} onClose={onClose} width={560}>
      <section className="settings-group">
        <h3>{t.settings.agents}</h3>
        <p className="modal-hint">{t.settings.agentsHint}</p>
        {agents.length === 0 && <p className="muted">{t.settings.agentNone}</p>}
        {agents.map((agent, i) => (
          <div className="agent-row" key={i}>
            <input
              value={agent.name}
              onChange={(e) => change(i, { name: e.target.value })}
              placeholder={t.settings.agentName}
              aria-label={t.settings.agentName}
              spellCheck={false}
              autoComplete="off"
            />
            <input
              value={agent.command}
              onChange={(e) => change(i, { command: e.target.value })}
              placeholder={t.settings.agentCommand}
              aria-label={t.settings.agentCommand}
              spellCheck={false}
              autoComplete="off"
            />
            <button className="ghost" disabled={i === 0} title={t.settings.agentMove("up", agent.name)} aria-label={t.settings.agentMove("up", agent.name)} onClick={() => move(i, -1)}>
              ↑
            </button>
            <button className="ghost" disabled={i === agents.length - 1} title={t.settings.agentMove("down", agent.name)} aria-label={t.settings.agentMove("down", agent.name)} onClick={() => move(i, 1)}>
              ↓
            </button>
            <button className="ghost" title={t.settings.agentRemove(agent.name)} aria-label={t.settings.agentRemove(agent.name)} onClick={() => setAgents(agents.filter((_, j) => j !== i))}>
              ×
            </button>
          </div>
        ))}
        <div>
          <button onClick={() => setAgents([...agents, { name: "", command: "" }])}>{t.settings.agentAdd}</button>
        </div>
      </section>
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
