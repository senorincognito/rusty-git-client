import { invoke } from "./invoke";

/** Starts emitting `repo-changed` events when HEAD, the index or refs change. */
export const watchRepo = (path: string) => invoke<void>("watch_repo", { path });
export const unwatchRepo = () => invoke<void>("unwatch_repo");
