/** Lines of a file, 1-based and inclusive. */
export interface LineRange {
  from: number;
  to: number;
}

/**
 * The text that points an AI harness in the terminal at a file or a part of it: `@path`, `@path#L12` or
 * `@path#L12-20` (the form Claude Code reads as a file mention). The one place to change when another tool wants
 * another form.
 */
export function fileReference(file: string, range: LineRange | null): string {
  if (!range) return `@${file}`;
  return range.from === range.to ? `@${file}#L${range.from}` : `@${file}#L${range.from}-${range.to}`;
}
