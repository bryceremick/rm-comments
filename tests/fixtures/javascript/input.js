// Line comment.
/**
 * JSDoc block.
 */
function main() {
  /* inline block */
  const url = "https://example.com"; // trailing
  const s = 'not // a comment';
  const t = `template /* not a comment */`;
  return [url, s, t];
}
