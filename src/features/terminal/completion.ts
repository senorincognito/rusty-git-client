// Branch and tag suggestions for the terminal. The shell owns the input line, so the panel keeps a shadow copy of it
// (`LineTracker`) from what xterm sends to the PTY, and only trusts it until something it can't follow happens.

/** git subcommands whose arguments are refs. */
const REF_COMMANDS = new Set([
  "checkout",
  "switch",
  "merge",
  "rebase",
  "cherry-pick",
  "branch",
  "diff",
  "log",
  "show",
  "reset",
  "revert",
  "tag",
  "push",
  "pull",
  "merge-base",
]);

/** Options after which the word is a new name, not an existing ref (`git checkout -b <new>`). */
const NEW_NAME_FLAGS = new Set(["-b", "-B", "-c", "-C"]);

const MAX_SUGGESTIONS = 8;

/**
 * Follows the text typed since the last Enter. Plain typing, Backspace, Ctrl+C and Ctrl+U are understood; any other
 * key (arrows, history, Tab, Ctrl+R, a paste with line breaks...) edits the shell's line in a way we can't see, so the
 * copy is marked invalid until the next Enter or Ctrl+C starts a fresh line.
 */
export class LineTracker {
  line = "";
  valid = true;

  reset() {
    this.line = "";
    this.valid = true;
  }

  /** Feeds one chunk from xterm's `onData`. */
  feed(data: string) {
    if (data === "\r" || data === "\x03") return this.reset();
    if (!this.valid) return;
    if (data === "\x15") this.line = "";
    else if (data === "\x7f") this.line = this.line.slice(0, -1);
    else if ([...data].every((c) => c >= " " && c !== "\x7f")) this.line += data;
    else this.valid = false;
  }
}

/** The word being completed: `prefix` is what has been typed of it. */
export interface CompletionContext {
  prefix: string;
}

/** Whether the line is a git command taking a ref, with its last word started (at least one character). */
export function completionContext(line: string): CompletionContext | null {
  const words = line.trimStart().split(/\s+/);
  if (words.length < 3 || words[0] !== "git" || !REF_COMMANDS.has(words[1])) return null;
  const word = words[words.length - 1];
  if (word === "" || word.startsWith("-")) return null;
  // Not a ref to suggest: the name of a branch about to be created.
  if (words.slice(2, -1).some((w) => NEW_NAME_FLAGS.has(w))) return null;
  // `main..feat`: the part after the range operator is the ref.
  const range = word.lastIndexOf("..");
  const prefix = range >= 0 ? word.slice(range + 2) : word;
  return prefix === "" ? null : { prefix };
}

/** Names starting with `prefix` first (case-insensitive), then those containing it; the order of `names` is kept. */
export function rankSuggestions(prefix: string, names: string[]): string[] {
  const p = prefix.toLowerCase();
  const starts: string[] = [];
  const contains: string[] = [];
  for (const name of names) {
    if (name === prefix) continue; // nothing left to complete
    const i = name.toLowerCase().indexOf(p);
    if (i === 0) starts.push(name);
    else if (i > 0) contains.push(name);
  }
  return [...starts, ...contains].slice(0, MAX_SUGGESTIONS);
}

/** What to type into the shell to turn `prefix` into `suggestion`. */
export function insertion(prefix: string, suggestion: string): string {
  return suggestion.startsWith(prefix) ? suggestion.slice(prefix.length) : "\x7f".repeat(prefix.length) + suggestion;
}
