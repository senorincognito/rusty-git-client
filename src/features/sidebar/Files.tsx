import { useCallback, useEffect, useMemo, useState } from "react";
import { getStatus, type ChangeKind } from "@/api/changes";
import { getFiles } from "@/api/files";
import ContextMenu from "@/components/ContextMenu";
import Section from "@/components/Section";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { t } from "@/i18n";
import { buildRows, FolderRow, leafIndent, useClosedFolders } from "./branchTree";
import { matchesFilter } from "./filter";

const byPath = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" });

/**
 * Every file of the working directory as a tree of folders (closed to start with, all open while a filter is
 * active). Clicking a file opens its content in the centre.
 */
export default function Files({
  path,
  refreshKey,
  selected,
  onSelect,
  onHistory,
  onNewFile,
  onDelete,
  filter,
}: {
  path: string;
  refreshKey: number;
  /** The file shown in the centre. */
  selected: string | null;
  onSelect: (file: string) => void;
  /** Show every commit that changed this file. */
  onHistory: (file: string) => void;
  /** Ask for a new file's name; the folder pre-fills it ("" = the repository root). */
  onNewFile: (folder: string) => void;
  /** Delete this file (RepoView asks first). */
  onDelete: (file: string) => void;
  /** Only files whose path matches are listed. */
  filter: string;
}) {
  const [files, setFiles] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Files are added and removed outside git's own files, which the watcher doesn't see.
  const [focusTick, setFocusTick] = useState(0);
  // What the working tree changed, by path (highlighted in the list), and the folders that hold such a file.
  const [changed, setChanged] = useState<Map<string, ChangeKind>>(new Map());
  const [menu, setMenu] = useState<
    { x: number; y: number; kind: "file"; file: string } | { x: number; y: number; kind: "folder"; folder: string } | { x: number; y: number; kind: "section" } | null
  >(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const folders = useClosedFolders();
  const start = useLatestRequest();
  const startStatus = useLatestRequest();

  useEffect(() => {
    const onFocus = () => setFocusTick((n) => n + 1);
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  useEffect(() => {
    const isCurrent = start();
    getFiles(path).then(
      (list) => {
        if (!isCurrent()) return;
        setFiles(list.sort(byPath));
        setError(null);
      },
      (e) => isCurrent() && setError(String(e)),
    );
  }, [path, refreshKey, focusTick, start]);

  useEffect(() => {
    const isCurrent = startStatus();
    getStatus(path).then(
      (list) => {
        if (!isCurrent()) return;
        // A conflict wins over the rest; otherwise the working-tree change over the staged one.
        const kinds = list.map((c): [string, ChangeKind] => [
          c.path,
          c.staged === "conflicted" || c.unstaged === "conflicted" ? "conflicted" : (c.unstaged ?? c.staged ?? "modified"),
        ]);
        setChanged(new Map(kinds));
      },
      () => isCurrent() && setChanged(new Map()),
    );
  }, [path, refreshKey, focusTick, startStatus]);

  const changedFolders = useMemo(() => {
    const set = new Set<string>();
    for (const file of changed.keys()) {
      for (let i = file.indexOf("/"); i !== -1; i = file.indexOf("/", i + 1)) set.add(file.slice(0, i));
    }
    return set;
  }, [changed]);

  const filtering = filter.trim() !== "";
  const shown = useMemo(() => files?.filter((f) => matchesFilter(filter, f)), [files, filter]);
  const rows = useMemo(
    () => (shown ? buildRows(shown, (f) => f, folders.closed, filtering, "", false) : []),
    [shown, folders.closed, filtering],
  );

  return (
    <Section
      title={t.files.title}
      count={shown?.length}
      action={{
        label: t.files.actions,
        active: menu?.kind === "section",
        onClick: (r) => (menu?.kind === "section" ? closeMenu() : setMenu({ x: r.left, y: r.bottom + 4, kind: "section" })),
      }}
    >
      {error && <p className="error side-msg">{error}</p>}
      {!files && !error && <p className="muted side-msg">{t.common.loading}</p>}
      {files?.length === 0 && <p className="muted side-msg">{t.files.none}</p>}
      {files && files.length > 0 && shown?.length === 0 && <p className="muted side-msg">{t.files.noMatch}</p>}
      <ul className="branchlist filelist-tree" role="tree">
        {rows.map((row) => {
          if (row.kind === "folder")
            return (
              <FolderRow
                key={"f:" + row.key}
                row={row}
                onToggle={folders.toggle}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setMenu({ x: e.clientX, y: e.clientY, kind: "folder", folder: row.key });
                }}
                countLabel={t.files.folderCount}
                className={changedFolders.has(row.key) ? "changed" : undefined}
              />
            );
          const file = row.item;
          return (
            <li
              key={file}
              style={leafIndent(row.depth)}
              className={
                (file === selected ? "selected" : "") +
                (changed.has(file) ? ` changed ch-${changed.get(file)}` : "") +
                (menu?.kind === "file" && menu.file === file ? " ctx" : "")
              }
              title={changed.has(file) ? `${file} (${t.files.changed(changed.get(file)!)})` : file}
              role="treeitem"
              aria-selected={file === selected}
              tabIndex={0}
              onClick={() => onSelect(file)}
              onContextMenu={(e) => {
                e.preventDefault();
                setMenu({ x: e.clientX, y: e.clientY, kind: "file", file });
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onSelect(file);
                }
              }}
            >
              <span className="bname">{row.label}</span>
            </li>
          );
        })}
      </ul>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={closeMenu}
          items={
            menu.kind === "file"
              ? [
                  { label: t.files.history, title: t.files.historyHint, onClick: () => onHistory(menu.file) },
                  { label: t.files.delete, title: t.files.deleteHint, danger: true, separatorBefore: true, onClick: () => onDelete(menu.file) },
                ]
              : menu.kind === "folder"
                ? [{ label: t.files.newFileHere, title: t.files.newFileHint, onClick: () => onNewFile(menu.folder) }]
                : [{ label: t.files.newFile, title: t.files.newFileHint, onClick: () => onNewFile("") }]
          }
        />
      )}
    </Section>
  );
}
