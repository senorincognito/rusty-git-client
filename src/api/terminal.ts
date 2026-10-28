import { invoke } from "./invoke";

export const termStart = (path: string, id: number, cols: number, rows: number) =>
  invoke<void>("term_start", { path, id, cols, rows });
export const termWrite = (id: number, data: string) => invoke<void>("term_write", { id, data });
export const termResize = (id: number, cols: number, rows: number) =>
  invoke<void>("term_resize", { id, cols, rows });
export const termStop = () => invoke<void>("term_stop");
