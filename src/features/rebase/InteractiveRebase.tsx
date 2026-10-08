import { useCallback, useEffect, useRef, useState } from "react";
import { applyRebase, getRebasePlan, type RebaseCommit, type RebasePlan, type RebaseStep } from "@/api/history";
import ContextMenu from "@/components/ContextMenu";
import Modal from "@/components/Modal";
import { followSelection } from "@/hooks/followSelection";
import { fill, t } from "@/i18n";
import "./InteractiveRebase.scss";

const dateFmt = new Intl.DateTimeFormat(undefined, { year: "numeric", month: "short", day: "numeric" });
const summaryOf = (message: string) => message.split("\n", 1)[0];
// The shortcut modifier: Cmd on macOS, Ctrl elsewhere.
const isMac = /Mac/.test(navigator.userAgent);

type Action = "pick" | "reword" | "squash" | "drop";

const ACTIONS: { value: Action; label: string; title: string }[] = [
  { value: "pick", label: t.rebase.pick, title: t.rebase.pickHint },
  { value: "reword", label: t.rebase.reword, title: t.rebase.rewordHint },
  { value: "squash", label: t.rebase.squash, title: t.rebase.squashHint },
  { value: "drop", label: t.rebase.drop, title: t.rebase.dropHint },
];

/**
 * The interactive-rebase screen, shown in place of the sidebar and the graph: every commit after `base`
 * on the current branch, newest first, each with an action. "Start rebase" applies them in one go.
 */
export default function InteractiveRebase({
  path,
  base,
  selectedId,
  diffOpen,
  onSelectCommit,
  onCancel,
  onApplied,
}: {
  path: string;
  base: { id: string; shortId: string };
  /** The commit whose changed files are shown in the right panel. */
  selectedId: string | null;
  /** A file diff is open on top of this screen: Escape belongs to it. */
  diffOpen: boolean;
  onSelectCommit: (commit: { id: string; shortId: string }) => void;
  onCancel: () => void;
  onApplied: () => void;
}) {
  const [plan, setPlan] = useState<RebasePlan | null>(null);
  // The commit ids in the order they are shown (newest first); "move up/down" changes it.
  const [order, setOrder] = useState<string[]>([]);
  // New messages of the commits set to "reword", and the commits set to "squash" (every other commit is a "pick").
  const [reworded, setReworded] = useState<Record<string, string>>({});
  const [squashed, setSquashed] = useState<Record<string, true>>({});
  const [dropped, setDropped] = useState<Record<string, true>>({});
  // The commit whose message is being edited in the popup.
  const [editing, setEditing] = useState<RebaseCommit | null>(null);
  // The row a right-click menu is open on.
  const [menu, setMenu] = useState<{ x: number; y: number; index: number } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let stale = false;
    setPlan(null);
    setOrder([]);
    setReworded({});
    setSquashed({});
    setDropped({});
    setEditing(null);
    setMenu(null);
    setError(null);
    getRebasePlan(path, base.id)
      .then((p) => {
        if (stale) return;
        setPlan(p);
        setOrder(p.commits.map((c) => c.id));
      })
      .catch((e) => !stale && setError(String(e)));
    return () => {
      stale = true;
    };
  }, [path, base.id]);

  // Escape cancels the rebase, unless it already means something else (a field, the popup, a menu).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented || e.isComposing || busy || diffOpen) return;
      const target = e.target as HTMLElement | null;
      if (target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))) return;
      if (document.querySelector(".ctxmenu, .modal-backdrop")) return;
      onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel, busy, diffOpen]);

  // The commits in the shown order (newest first).
  const planCommits = plan?.commits ?? [];
  const byId = new Map(planCommits.map((c) => [c.id, c]));
  const commits = order.flatMap((id) => byId.get(id) ?? []);
  // Where a commit sits compared with the plan: from the first (oldest) difference on, the history is replayed.
  const moved = commits.map((c, i) => c.id !== planCommits[i]?.id);
  const reordered = moved.some(Boolean);
  const rewordCount = Object.keys(reworded).length;
  const squashCount = Object.keys(squashed).length;
  const dropCount = Object.keys(dropped).length;
  // The list is newest first, so the commit a squash melds into is the next one down that is not squashed itself.
  const squashTarget = (index: number) => {
    let at = index + 1;
    while (at < commits.length && commits[at].id in squashed) at++;
    return commits[at];
  };
  // The commits that change directly, and (the oldest of them) where the rebuilding starts.
  const touched = new Set<string>([...Object.keys(reworded), ...Object.keys(dropped)]);
  let oldest = commits.reduce((at, c, i) => (c.id in reworded || c.id in dropped || moved[i] ? i : at), -1);
  commits.forEach((c, i) => moved[i] && touched.add(c.id));
  commits.forEach((c, i) => {
    if (!(c.id in squashed)) return;
    touched.add(c.id);
    const target = squashTarget(i);
    if (target) {
      touched.add(target.id);
      oldest = Math.max(oldest, commits.indexOf(target));
    }
  });
  // Everything from the oldest change up to the tip gets a new id.
  const rewritten = oldest < 0 ? [] : commits.slice(0, oldest + 1);
  const pushedRewritten = rewritten.filter((c) => c.pushed).length;
  const changeCount = rewordCount + squashCount + dropCount + (reordered ? 1 : 0);

  // Why an action isn't available for the commit at `index` (null: it is). Shared by the select and the menu.
  const squashBlockedFor = (index: number) => {
    const c = commits[index];
    // A squash needs a commit before it in this rebase, and a merge commit can't be melded away.
    const olderTarget = commits.slice(index + 1).find((o) => !(o.id in squashed));
    return index === commits.length - 1
      ? t.rebase.squashOldest
      : c.isMerge
        ? t.rebase.squashMerge
        : olderTarget && olderTarget.id in dropped
          ? t.rebase.squashIntoDropped
          : null;
  };
  // A commit that others are squashed into can't be dropped (the commit just above it is squashed).
  const dropBlockedFor = (index: number) =>
    index > 0 && commits[index - 1].id in squashed ? t.rebase.dropSquashed : null;

  // Moving can leave an action that no longer fits (a squash that is now the oldest commit, ...).
  const problem = (() => {
    for (let i = 0; i < commits.length; i++) {
      const reason = (commits[i].id in squashed && squashBlockedFor(i)) || (commits[i].id in dropped && dropBlockedFor(i));
      if (reason) return t.rebase.invalid(commits[i].shortId, reason);
    }
    return null;
  })();
  const canStart = !busy && plan !== null && changeCount > 0 && problem === null;

  const moveCommit = useCallback(
    (index: number, delta: -1 | 1) =>
      setOrder((o) => {
        if (index + delta < 0 || index + delta >= o.length) return o;
        const next = [...o];
        [next[index], next[index + delta]] = [next[index + delta], next[index]];
        return next;
      }),
    [],
  );

  // Arrow keys: select the commit above/below (with nothing selected, ↓ starts at the top commit and ↑ at the bottom one); with Ctrl
  // (Cmd on macOS) they move the selected commit instead. Typing in a field, an open menu or popup, a file
  // diff on top of this screen and a running rebase all leave the arrows alone.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.key !== "ArrowUp" && e.key !== "ArrowDown") || e.defaultPrevented || e.isComposing) return;
      if (e.shiftKey || e.altKey || busy || diffOpen || commits.length === 0) return;
      const target = e.target as HTMLElement | null;
      if (target && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))) return;
      if (document.querySelector(".ctxmenu, .modal-backdrop")) return;
      // The other platform's modifier is not ours (Ctrl+arrows belong to the system on a Mac).
      if (isMac ? e.ctrlKey : e.metaKey) return;

      const up = e.key === "ArrowUp";
      const index = commits.findIndex((c) => c.id === selectedId);
      e.preventDefault(); // keep the list from scrolling by itself
      if (isMac ? e.metaKey : e.ctrlKey) {
        if (index >= 0) moveCommit(index, up ? -1 : 1); // up = newer, like the list
        return;
      }
      const next = index < 0 ? (up ? commits.length - 1 : 0) : Math.min(Math.max(index + (up ? -1 : 1), 0), commits.length - 1);
      if (next !== index) onSelectCommit({ id: commits[next].id, shortId: commits[next].shortId });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [commits, selectedId, busy, diffOpen, moveCommit, onSelectCommit]);

  // Keep the selected row in view after the selection or the order changes.
  const list = useRef<HTMLOListElement>(null);
  useEffect(() => {
    followSelection(list.current, ".rebaserow.selected");
  }, [selectedId, order]);

  const setAction = (c: RebaseCommit, action: Action) => {
    if (action === "reword") {
      setEditing(c); // the commit only becomes "reword" once the popup is confirmed
      return;
    }
    // A commit has exactly one action: clear the others, then set this one.
    const without = (r: Record<string, true>, on: boolean) => {
      const next = { ...r };
      if (on) next[c.id] = true;
      else delete next[c.id];
      return next;
    };
    setReworded((r) => {
      const next = { ...r };
      delete next[c.id];
      return next;
    });
    setSquashed((q) => without(q, action === "squash"));
    setDropped((d) => without(d, action === "drop"));
  };

  const start = async () => {
    if (!plan) return;
    setBusy(true);
    setError(null);
    try {
      const steps: RebaseStep[] = [
        ...Object.entries(reworded).map(([id, message]) => ({ id, action: "reword" as const, message })),
        ...Object.keys(squashed).map((id) => ({ id, action: "squash" as const })),
        ...Object.keys(dropped).map((id) => ({ id, action: "drop" as const })),
      ];
      await applyRebase(path, base.id, plan.headId, steps, order);
      onApplied();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const cancelButton = (
    <button className="secondary" onClick={onCancel} disabled={busy}>
      {t.rebase.cancel}
    </button>
  );

  return (
    <section className="rebaseview">
      <header className="rebasehead">
        <div className="rebasetitle">
          <strong>{t.rebase.title}</strong>
          <span className="muted">
            {fill(t.rebase.onto, { base: <code>{base.shortId}</code> })}
            {plan && t.rebase.count(commits.length)}
          </span>
        </div>
        {cancelButton}
      </header>

      <div className="rebasebody">
        {!plan && !error && <p className="muted rebase-msg">{t.common.loading}</p>}
        <ol className="rebaselist" ref={list}>
          {commits.map((c, index) => {
            const action: Action =
              c.id in dropped ? "drop" : c.id in squashed ? "squash" : c.id in reworded ? "reword" : "pick";
            const message = reworded[c.id] ?? c.message;
            const squashBlocked = squashBlockedFor(index);
            const dropBlocked = dropBlockedFor(index);
            return (
              <li
                key={c.id}
                className={
                  "rebaserow " + action + (c.id === selectedId ? " selected" : "") + (menu?.index === index ? " ctx" : "")
                }
                title={t.rebase.rowHint}
                tabIndex={0}
                // Clicking the row (not its select or buttons) shows the commit's files in the right panel.
                onClick={(e) => {
                  if (!(e.target as HTMLElement).closest("select, button")) {
                    onSelectCommit({ id: c.id, shortId: c.shortId });
                  }
                }}
                onKeyDown={(e) => {
                  if (e.target === e.currentTarget && (e.key === "Enter" || e.key === " ")) {
                    e.preventDefault();
                    onSelectCommit({ id: c.id, shortId: c.shortId });
                  }
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  if (!busy) setMenu({ x: e.clientX, y: e.clientY, index });
                }}
              >
                <select
                  className="rebase-action"
                  value={action}
                  disabled={busy}
                  aria-label={t.rebase.actionFor(c.shortId)}
                  title={ACTIONS.find((a) => a.value === action)?.title}
                  onChange={(e) => setAction(c, e.target.value as Action)}
                >
                  {ACTIONS.map((a) => (
                    <option
                      key={a.value}
                      value={a.value}
                      disabled={
                        (a.value === "squash" && squashBlocked !== null) || (a.value === "drop" && dropBlocked !== null)
                      }
                      title={
                        a.value === "squash" && squashBlocked
                          ? squashBlocked
                          : a.value === "drop" && dropBlocked
                            ? dropBlocked
                            : a.title
                      }
                    >
                      {a.label}
                    </option>
                  ))}
                </select>
                <code className="rebase-id">{c.shortId}</code>
                <span className="rebase-summary" title={message}>
                  {summaryOf(message)}
                </span>
                {action === "squash" && (
                  <span className="rebase-into">{t.rebase.squashInto(squashTarget(index)?.shortId ?? "")}</span>
                )}
                {action === "drop" && <span className="rebase-gone">{t.rebase.dropped}</span>}
                {moved[index] && (
                  <span className="rebase-tag moved" title={t.rebase.movedHint}>
                    {t.rebase.moved}
                  </span>
                )}
                {action === "reword" && (
                  <button className="ghost rebase-edit" onClick={() => setEditing(c)} disabled={busy}>
                    {t.rebase.editMessage}
                  </button>
                )}
                {c.isMerge && <span className="rebase-tag">{t.rebase.merge}</span>}
                {c.pushed && (
                  <span className="rebase-tag pushed" title={t.rebase.pushedHint}>
                    {t.rebase.pushed}
                  </span>
                )}
                <span className="rebase-author">{c.author}</span>
                <span className="rebase-date">{dateFmt.format(new Date(c.time * 1000))}</span>
              </li>
            );
          })}
        </ol>
        {plan && (
          <div className="rebasebase">
            <code>{base.shortId}</code> <span className="muted">{t.rebase.base}</span>
          </div>
        )}
      </div>

      <footer className="rebasefoot">
        <div className="rebase-info">
          {error && <p className="error">{error}</p>}
          {!error && changeCount === 0 && plan && <p className="muted">{t.rebase.chooseAction}</p>}
          {problem && <p className="error">{problem}</p>}
          {changeCount > 0 && (
            <p className="muted">
              {t.rebase.summary(rewordCount, squashCount, dropCount, reordered, rewritten.length - touched.size)}
            </p>
          )}
          {(dropCount > 0 || reordered) && <p className="muted">{t.rebase.replayNote}</p>}
          {pushedRewritten > 0 && (
            <p className="warn">
              {t.rebase.pushedWarning(pushedRewritten)}
            </p>
          )}
        </div>
        {cancelButton}
        <button className="primary" onClick={start} disabled={!canStart}>
          {busy ? t.rebase.starting : t.rebase.start}
        </button>
      </footer>

      {menu && commits[menu.index] && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={closeMenu}
          items={[
            { label: t.rebase.menuReword, title: t.rebase.rewordHint, onClick: () => setAction(commits[menu.index], "reword") },
            {
              label: t.rebase.menuSquash,
              disabled: squashBlockedFor(menu.index) !== null,
              title: squashBlockedFor(menu.index) ?? t.rebase.squashHint,
              onClick: () => setAction(commits[menu.index], "squash"),
            },
            {
              label: t.rebase.menuDrop,
              danger: true,
              disabled: dropBlockedFor(menu.index) !== null,
              title: dropBlockedFor(menu.index) ?? t.rebase.dropHint,
              onClick: () => setAction(commits[menu.index], "drop"),
            },
            {
              label: t.rebase.menuMoveUp,
              separatorBefore: true,
              disabled: menu.index === 0,
              title: menu.index === 0 ? t.rebase.moveTopHint : t.rebase.moveUpHint,
              onClick: () => moveCommit(menu.index, -1),
            },
            {
              label: t.rebase.menuMoveDown,
              disabled: menu.index === commits.length - 1,
              title: menu.index === commits.length - 1 ? t.rebase.moveBottomHint : t.rebase.moveDownHint,
              onClick: () => moveCommit(menu.index, 1),
            },
          ]}
        />
      )}

      {editing && (
        <RewordDialog
          commit={editing}
          initial={reworded[editing.id] ?? editing.message}
          onCancel={() => setEditing(null)}
          onUpdate={(message) => {
            const id = editing.id;
            for (const set of [setSquashed, setDropped]) {
              set((q) => {
                const next = { ...q };
                delete next[id]; // rewording replaces any other action
                return next;
              });
            }
            setReworded((r) => {
              const next = { ...r };
              // Back to the original message: nothing to reword.
              if (message.trim() === editing.message.trim()) delete next[id];
              else next[id] = message;
              return next;
            });
            setEditing(null);
          }}
        />
      )}
    </section>
  );
}

/** Popup that edits one commit's message for the rebase. */
function RewordDialog({
  commit,
  initial,
  onCancel,
  onUpdate,
}: {
  commit: RebaseCommit;
  initial: string;
  onCancel: () => void;
  onUpdate: (message: string) => void;
}) {
  const [message, setMessage] = useState(initial);
  const box = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    box.current?.focus();
    box.current?.select();
  }, []);

  const canUpdate = message.trim() !== "";

  return (
    <Modal title={t.rebase.rewordTitle(commit.shortId)} onClose={onCancel} width={560}>
      <textarea
        spellCheck={false}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        ref={box}
        className="reword-text"
        value={message}
        rows={8}
        aria-label={t.rebase.messageLabel}
        onChange={(e) => setMessage(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && canUpdate) onUpdate(message);
        }}
      />
      {!canUpdate && <p className="error">{t.rebase.emptyMessage}</p>}
      <p className="modal-hint">{t.rebase.rewordAppliesLater}</p>
      <div className="modal-actions">
        <button className="secondary" onClick={onCancel}>
          {t.common.cancel}
        </button>
        <button className="primary" onClick={() => onUpdate(message)} disabled={!canUpdate}>
          {t.rebase.updateMessage}
        </button>
      </div>
    </Modal>
  );
}
