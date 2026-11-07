import { useCallback, useEffect, useState } from "react";
import { getTags, type TagInfo } from "@/api/graph";
import ContextMenu from "@/components/ContextMenu";
import Section from "@/components/Section";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { t } from "@/i18n";
import { matchesFilter } from "./filter";

/**
 * Every tag of the repository, newest first. Clicking one shows its commit in the right panel; right-clicking
 * offers to copy its name or delete it. Tags are made from the graph's context menu.
 */
export default function Tags({
  path,
  refreshKey,
  selectedId,
  onSelect,
  onDelete,
  filter,
}: {
  path: string;
  refreshKey: number;
  /** The commit whose details are open, so the tags on it can be highlighted. */
  selectedId: string | null;
  onSelect: (commit: { id: string; shortId: string }) => void;
  /** Delete the tag by name (RepoView asks first). */
  onDelete: (name: string) => void;
  /** Only tags whose name or annotation matches are listed. */
  filter: string;
}) {
  const [tags, setTags] = useState<TagInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; tag: TagInfo } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const start = useLatestRequest();

  useEffect(() => {
    const isCurrent = start();
    getTags(path).then(
      (list) => {
        if (!isCurrent()) return;
        setTags(list);
        setError(null);
      },
      (e) => isCurrent() && setError(String(e)),
    );
  }, [path, refreshKey, start]);

  const shown = tags?.filter((tag) => matchesFilter(filter, tag.name, tag.message ?? ""));

  return (
    <Section title={t.tags.title} count={shown?.length}>
      {error && <p className="error side-msg">{error}</p>}
      {tags?.length === 0 && <p className="muted side-msg">{t.tags.none}</p>}
      {tags && tags.length > 0 && shown?.length === 0 && <p className="muted side-msg">{t.tags.noMatch}</p>}
      <ul className="branchlist taglist">
        {shown?.map((tag) => (
          <li
            key={tag.name}
            className={(tag.id === selectedId ? "selected" : "") + (menu?.tag.name === tag.name ? " ctx" : "")}
            title={t.tags.tooltip(tag)}
            role="button"
            tabIndex={0}
            onClick={() => onSelect({ id: tag.id, shortId: tag.shortId })}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onSelect({ id: tag.id, shortId: tag.shortId });
              }
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ x: e.clientX, y: e.clientY, tag });
            }}
          >
            <span className="bname">{tag.name}</span>
            {tag.annotated && <span className="tag-kind">{t.tags.annotated}</span>}
            <code className="tag-id">{tag.shortId}</code>
          </li>
        ))}
      </ul>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={closeMenu}
          items={[
            {
              label: t.tags.show,
              title: t.tags.showHint,
              onClick: () => onSelect({ id: menu.tag.id, shortId: menu.tag.shortId }),
            },
            {
              label: t.tags.copy,
              onClick: () => navigator.clipboard?.writeText(menu.tag.name).catch(() => {}),
            },
            {
              label: t.tags.delete,
              danger: true,
              separatorBefore: true,
              title: t.tags.deleteHint,
              onClick: () => onDelete(menu.tag.name),
            },
          ]}
        />
      )}
    </Section>
  );
}
