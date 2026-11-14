import { invoke } from "./invoke";

export interface FileContent {
  size: number;
  /** `content` is only there for "text". */
  kind: "text" | "binary" | "tooLarge";
  content: string | null;
}

/** Every file of the working directory (tracked, plus untracked that are not ignored), as `/`-separated paths. */
export const getFiles = (path: string) => invoke<string[]>("get_files", { path });

export const getFileContent = (path: string, file: string) => invoke<FileContent>("get_file_content", { path, file });

export const createFile = (path: string, file: string, content: string) =>
  invoke<void>("create_file_cmd", { path, file, content });

export const saveFile = (path: string, file: string, content: string) =>
  invoke<void>("save_file_cmd", { path, file, content });

export const deleteFile = (path: string, file: string) => invoke<void>("delete_file_cmd", { path, file });

export interface LineBlame {
  id: string;
  shortId: string;
  author: string;
  /** Unix seconds. */
  time: number;
  summary: string;
}

/** The commit that last changed one line (1-based) of the file as it is on disk; null when no commit has it yet. */
export const getLineBlame = (path: string, file: string, line: number) =>
  invoke<LineBlame | null>("get_line_blame", { path, file, line });
