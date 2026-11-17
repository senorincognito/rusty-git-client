import type { AgentStatus } from "@/hooks/useTerminalSessions";
import { t } from "@/i18n";
import "./AgentDot.scss";

/** A small status dot for an agent's terminal session (see `useTerminalSessions`). */
export default function AgentDot({ agent, status }: { agent: string; status: AgentStatus }) {
  const label = t.agents.status(agent, status);
  return <span className={`agentdot ${status}`} role="img" title={label} aria-label={label} />;
}
