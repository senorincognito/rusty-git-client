import type { Divergence } from "@/api/sync";
import Modal from "@/components/Modal";
import { fill, t } from "@/i18n";
import { CommitList } from "./PullDialog";
import "./PullDialog.scss";

/**
 * Shown when Push is pressed on a branch that has diverged from its upstream: a normal push would be rejected.
 * Force push (with lease) is offered instead; Cancel, the default, aborts without touching anything.
 */
export default function PushDialog({
  divergence: d,
  onForcePush,
  onCancel,
}: {
  divergence: Divergence;
  onForcePush: () => void;
  onCancel: () => void;
}) {
  return (
    <Modal title={t.pushDialog.title} onClose={onCancel} width={640}>
      <p className="modal-lead">
        {fill(t.pushDialog.lead, { branch: <strong>{d.branch}</strong>, upstream: <strong>{d.upstream}</strong> })}
      </p>
      <div className="div-cols">
        <CommitList title={t.pullDialog.onlyLocal} commits={d.ahead} total={d.aheadTotal} />
        <CommitList title={t.pullDialog.onlyRemote(d.upstream)} commits={d.behind} total={d.behindTotal} />
      </div>
      <p className="modal-hint">
        {fill(t.pushDialog.hint, {
          force: <strong>{t.pushDialog.forcePush}</strong>,
          upstream: d.upstream,
          discarded: t.pushDialog.discarded(d.behindTotal, d.upstream),
        })}
      </p>
      <div className="modal-actions">
        <button className="secondary" onClick={onForcePush}>
          {t.pushDialog.forcePush}
        </button>
        <button className="primary" onClick={onCancel} autoFocus>
          {t.common.cancel}
        </button>
      </div>
    </Modal>
  );
}
