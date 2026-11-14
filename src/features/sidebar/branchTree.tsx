import { useCallback, useState } from "react";
import { t } from "@/i18n";

/** One line of a branch list shown as folders: a prefix before a "/" is a folder, the rest is the branch's label. */
export type TreeRow<T> =
  | { kind: "folder"; key: string; label: string; depth: number; count: number; open: boolean }
  | { kind: "leaf"; item: T; label: string; depth: number };

type Node<T> = { folders: Map<string, Node<T>>; leaves: { item: T; label: string }[]; count: number };

/**
 * Flattens names into rows: folders first, then branches, each in the order given. A closed folder hides its
 * contents; `openAll` (an active filter) shows everything. `keyPrefix` keeps folders of different lists apart.
 * `toggled` holds the folders the user flipped from their default: closed ones, or with `defaultOpen` false, open ones.
 */
export function buildRows<T>(
  items: T[],
  nameOf: (item: T) => string,
  toggled: Set<string>,
  openAll: boolean,
  keyPrefix = "",
  defaultOpen = true,
): TreeRow<T>[] {
  const root: Node<T> = { folders: new Map(), leaves: [], count: 0 };
  for (const item of items) {
    const parts = nameOf(item).split("/");
    let node = root;
    for (const part of parts.slice(0, -1)) {
      let next = node.folders.get(part);
      if (!next) node.folders.set(part, (next = { folders: new Map(), leaves: [], count: 0 }));
      next.count++;
      node = next;
    }
    node.leaves.push({ item, label: parts[parts.length - 1] });
  }
  const rows: TreeRow<T>[] = [];
  const walk = (node: Node<T>, path: string, depth: number) => {
    for (const [name, child] of node.folders) {
      const key = keyPrefix + path + name;
      const open = openAll || toggled.has(key) !== defaultOpen;
      rows.push({ kind: "folder", key, label: name, depth, count: child.count, open });
      if (open) walk(child, path + name + "/", depth + 1);
    }
    for (const { item, label } of node.leaves) rows.push({ kind: "leaf", item, label, depth });
  };
  walk(root, "", 0);
  return rows;
}

/** Which folders the user closed (not persisted). */
export function useClosedFolders() {
  const [closed, setClosed] = useState<Set<string>>(new Set());
  const toggle = useCallback(
    (key: string) =>
      setClosed((c) => {
        const next = new Set(c);
        if (!next.delete(key)) next.add(key);
        return next;
      }),
    [],
  );
  return { closed, toggle };
}

/** The list row of a folder: click to open or close it. */
export function FolderRow({
  row,
  onToggle,
  countLabel = t.localBranches.folderCount,
  className,
  onContextMenu,
}: {
  row: Extract<TreeRow<unknown>, { kind: "folder" }>;
  onToggle: (key: string) => void;
  /** Words for the count badge's aria-label. */
  countLabel?: (n: number) => string;
  className?: string;
  onContextMenu?: (e: React.MouseEvent) => void;
}) {
  return (
    <li
      className={className ? `folder ${className}` : "folder"}
      style={{ paddingLeft: 12 + row.depth * 14 }}
      title={row.label}
      role="treeitem"
      aria-expanded={row.open}
      onClick={() => onToggle(row.key)}
      onContextMenu={onContextMenu}
    >
      <span className="chev">{row.open ? "▾" : "▸"}</span>
      <span className="bname">{row.label}</span>
      <span className="sync" aria-label={countLabel(row.count)}>
        {row.count}
      </span>
    </li>
  );
}

/** Indent of a branch row at the given depth (a top-level branch lines up with a folder's name). */
export const leafIndent = (depth: number) => ({ paddingLeft: 28 + depth * 14 });
