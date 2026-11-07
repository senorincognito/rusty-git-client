import { invoke } from "./invoke";

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

export interface SearchHit {
  id: string;
  /** Position among the graph's commit rows (the uncommitted-changes row not counted). */
  index: number;
}

export interface SearchResult {
  /** In graph order, newest first. */
  hits: SearchHit[];
  /** More commits matched than `hits` holds. */
  truncated: boolean;
}

/** Commits whose message or tag name contains `query` (ignoring case) or whose id starts with it (4+ hex digits). */
export const searchCommits = (path: string, query: string) =>
  invoke<SearchResult>("search_commits", { path, query });

export interface TagInfo {
  name: string;
  /** The commit the tag points at (an annotated tag is followed to it). */
  id: string;
  shortId: string;
  annotated: boolean;
  /** The annotation's text of an annotated tag. */
  message: string | null;
  /** Unix seconds: when an annotated tag was made, else the commit's time. */
  time: number;
}

/** Every tag of the repository, newest first. */
export const getTags = (path: string) => invoke<TagInfo[]>("get_tags", { path });

/** Tags a commit (hex id): lightweight, or annotated when `message` has text. Rejects when the name is taken or invalid. */
export const createTag = (path: string, id: string, name: string, message: string | null) =>
  invoke<void>("create_tag_cmd", { path, id, name, message });

/** Deletes a local tag. A pushed copy on the server stays. */
export const deleteTag = (path: string, name: string) => invoke<void>("delete_tag_cmd", { path, name });
