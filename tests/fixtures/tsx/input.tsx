// Line comment.
export function App() {
  /* block */
  const title = "hello // world"; // trailing
  return (
    <div>
      {/* jsx comment */}
      <span>{title}</span>
    </div>
  );
}
