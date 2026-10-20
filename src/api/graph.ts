import { invoke } from "@tauri-apps/api/core";

export interface RefLabel {
  name: string;
  kind: "branch" | "remote" | "tag" | "stash";
  isHead: boolean;
}

export interface Edge {
  col: number;
  color: number;
  /** Drawn dashed: the lane from the uncommitted-changes row down to the commit it sits on. */
  dashed: boolean;
}

export interface GraphRow {
  id: string;
  shortId: string;
  summary: string;
  author: string;
  email: string;
  /** Unix seconds. */
  time: number;
  parents: string[];
  /** HEAD or an ancestor of it: can be rewritten on the current branch. */
  onHead: boolean;
  /** A stash: drawn as a hollow node hanging off the commit it was made on. */
  isStash: boolean;
  /** The pseudo commit standing for uncommitted changes (id "WIP"). */
  isWip: boolean;
  refs: RefLabel[];
  col: number;
  color: number;
  top: Edge[];
  through: Edge[];
  bottom: Edge[];
}

export interface Graph {
  rows: GraphRow[];
  maxLanes: number;
  hasMore: boolean;
}

/** `limit` commits from row `skip` (the uncommitted-changes row comes on top of the first page and does not count). */
export const getGraph = (path: string, skip: number, limit: number) =>
  invoke<Graph>("get_graph", { path, skip, limit });
