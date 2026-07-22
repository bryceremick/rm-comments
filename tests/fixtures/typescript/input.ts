// Line comment.
/** JSDoc. */
export function add(a: number, b: number): number {
  /* block */
  const note: string = "// not a comment"; // trailing
  return a + b; // sum
}
