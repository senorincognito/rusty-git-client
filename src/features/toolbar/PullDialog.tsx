import type { BriefCommit, Divergence } from "@/api/sync";
import Modal from "@/components/Modal";
import { fill, t } from "@/i18n";
import "./PullDialog.scss";

export function CommitList({ title, commits, total }: { title: string; commits: BriefCommit[]; total: number }) {
  return (
    <section>
      <h3>
        {title} <span className="count">{total}</span>
      </h3>
      <ul>
        {commits.map((c) => (
          <li key={c.shortId} title={c.summary}>
            <code>{c.shortId}</code> <span>{c.summary}</span>
          </li>
        ))}
        {total > commits.length && <li className="muted">{t.pullDialog.more(total - commits.length)}</li>}
      </ul>
    </section>
  );
}

/**
 * Shown when a pull can't fast-forward because both sides have new commits. Merge is the
 * default (Enter): it leaves every commit as it is. Rebase rewrites the local commits.
 */
export default function PullDialog({
  divergence: d,
  onMerge,
  onRebase,
  onCancel,
}: {
  divergence: Divergence;
  onMerge: () => void;
  onRebase: () => void;
  onCancel: () => void;
}) {
  return (
    <Modal title={t.pullDialog.title} onClose={onCancel} width={640}>
      <p className="modal-lead">
        {fill(t.pullDialog.lead, { branch: <strong>{d.branch}</strong>, upstream: <strong>{d.upstream}</strong> })}
      </p>
      <div className="div-cols">
        <CommitList title={t.pullDialog.onlyLocal} commits={d.ahead} total={d.aheadTotal} />
        <CommitList title={t.pullDialog.onlyRemote(d.upstream)} commits={d.behind} total={d.behindTotal} />
      </div>
      <p className="modal-hint">
        {fill(t.pullDialog.hint, {
          merge: <strong>{t.pullDialog.merge}</strong>,
          rebase: <strong>{t.pullDialog.rebase}</strong>,
          upstream: d.upstream,
        })}
      </p>
      <div className="modal-actions">
        <button className="secondary" onClick={onCancel}>
          {t.common.cancel}
        </button>
        <button className="secondary" onClick={onRebase}>
          {t.pullDialog.rebase}
        </button>
        <button className="primary" onClick={onMerge} autoFocus>
          {t.pullDialog.merge}
        </button>
      </div>
    </Modal>
  );
}
