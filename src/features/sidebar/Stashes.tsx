import { useCallback, useEffect, useState } from "react";
import { confirmDialog } from "@/api/dialog";
import { applyStash, dropAllStashes, dropStash, getStashes, popStash, type StashEntry } from "@/api/stash";
import ContextMenu from "@/components/ContextMenu";
import Section from "@/components/Section";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { matchesFilter } from "./filter";
import { useWorkingChangeCount } from "@/hooks/useWorkingChangeCount";
import { t } from "@/i18n";

/**
 * The stashes of the repository, newest first. Clicking one shows its changes in the right panel;
 * right-clicking offers to pop or delete it.
 */
export default function Stashes({
  path,
  refreshKey,
  selectedId,
  onSelect,
  onPopped,
  onApplied,
  onDropped,
  onAllDropped,
  filter,
}: {
  path: string;
  refreshKey: number;
  /** The commit whose details are open, so the matching stash can be highlighted. */
  selectedId: string | null;
  onSelect: (stash: { id: string; shortId: string }) => void;
  /** A stash was applied and removed (by its commit id). */
  onPopped: (id: string) => void;
  /** A stash was applied and kept. */
  onApplied: () => void;
  /** A stash was deleted without being applied (by its commit id). */
  onDropped: (id: string) => void;
  /** Every stash was deleted (by their commit ids). */
  onAllDropped: (ids: string[]) => void;
  /** Only stashes whose message or "stash@{n}" matches are listed. */
  filter: string;
}) {
  const [stashes, setStashes] = useState<StashEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; stash: StashEntry } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const [popping, setPopping] = useState(false);
  // The "⋯" menu in the section's headline.
  const [sectionMenu, setSectionMenu] = useState<{ x: number; y: number } | null>(null);
  const closeSectionMenu = useCallback(() => setSectionMenu(null), []);
  const shown = stashes?.filter((s) => matchesFilter(filter, s.message, `stash@{${s.index}}`));
  const workingChanges = useWorkingChangeCount(path, refreshKey);
  const start = useLatestRequest();

  const refresh = useCallback(async () => {
    const isCurrent = start();
    try {
      const list = await getStashes(path);
      if (!isCurrent()) return;
      setStashes(list);
      setError(null);
    } catch (e) {
      if (isCurrent()) setError(String(e));
    }
  }, [path, start]);

  useEffect(() => {
    refresh();
  }, [refresh, refreshKey]);

  const pop = async (stash: StashEntry) => {
    setPopping(true);
    setError(null);
    try {
      await popStash(path, stash.id);
      onPopped(stash.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setPopping(false);
    }
  };

  const apply = async (stash: StashEntry) => {
    setPopping(true);
    setError(null);
    try {
      await applyStash(path, stash.id);
      onApplied();
    } catch (e) {
      setError(String(e));
    } finally {
      setPopping(false);
    }
  };

  const dropAll = async () => {
    if (!stashes || stashes.length === 0) return;
    const ok = await confirmDialog(
      t.stashes.deleteAllConfirm(stashes.map((s) => `stash@{${s.index}}  ${s.message}`)),
      t.stashes.deleteAll(stashes.length),
      true,
      t.common.delete,
    );
    if (!ok) return;
    setPopping(true);
    setError(null);
    try {
      await dropAllStashes(path);
      onAllDropped(stashes.map((s) => s.id));
    } catch (e) {
      setError(String(e));
    } finally {
      setPopping(false);
    }
  };

  const drop = async (stash: StashEntry) => {
    const ok = await confirmDialog(
      t.stashes.deleteConfirm(`stash@{${stash.index}}`, stash.message),
      t.stashes.delete,
      true,
      t.common.delete,
    );
    if (!ok) return;
    setPopping(true);
    setError(null);
    try {
      await dropStash(path, stash.id);
      onDropped(stash.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setPopping(false);
    }
  };

  return (
    <Section
      title={t.stashes.title}
      resizeKey="stashes"
      count={shown?.length}
      action={{
        label: t.stashes.actions,
        active: sectionMenu !== null,
        onClick: (r) => (sectionMenu ? closeSectionMenu() : setSectionMenu({ x: r.left, y: r.bottom + 4 })),
      }}
    >
      {error && <p className="error side-msg">{error}</p>}
      {stashes?.length === 0 && <p className="muted side-msg">{t.stashes.none}</p>}
      {stashes && stashes.length > 0 && shown?.length === 0 && <p className="muted side-msg">{t.stashes.noMatch}</p>}
      <ul className="branchlist stashlist">
        {shown?.map((s) => (
          <li
            key={s.id}
            className={(s.id === selectedId ? "selected" : "") + (menu?.stash.id === s.id ? " ctx" : "")}
            title={`${s.message}\n${new Date(s.time * 1000).toLocaleString()}`}
            role="button"
            tabIndex={0}
            onClick={() => onSelect({ id: s.id, shortId: s.shortId })}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onSelect({ id: s.id, shortId: s.shortId });
              }
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ x: e.clientX, y: e.clientY, stash: s });
            }}
          >
            <code className="stash-idx">{`stash@{${s.index}}`}</code>
            <span className="bname">{s.message}</span>
          </li>
        ))}
      </ul>
      {sectionMenu && (
        <ContextMenu
          x={sectionMenu.x}
          y={sectionMenu.y}
          onClose={closeSectionMenu}
          items={[
            {
              label: t.stashes.deleteAll(stashes?.length ?? 0),
              danger: true,
              disabled: popping || !stashes || stashes.length === 0,
              title: !stashes || stashes.length === 0 ? t.stashes.deleteAllNone : t.stashes.deleteAllHint,
              onClick: dropAll,
            },
          ]}
        />
      )}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={closeMenu}
          items={[
            {
              label: t.stashes.apply,
              disabled: popping || workingChanges > 0,
              title: workingChanges > 0 ? t.stashes.popNeedsClean : t.stashes.applyHint,
              onClick: () => apply(menu.stash),
            },
            {
              label: t.stashes.pop,
              disabled: popping || workingChanges > 0,
              title:
                workingChanges > 0
                  ? t.stashes.popNeedsClean
                  : t.stashes.popHint,
              onClick: () => pop(menu.stash),
            },
            {
              label: t.stashes.delete,
              danger: true,
              disabled: popping,
              separatorBefore: true,
              title: t.stashes.deleteHint,
              onClick: () => drop(menu.stash),
            },
          ]}
        />
      )}
    </Section>
  );
}
