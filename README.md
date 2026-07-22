# rm-comments

Remove **all** comments from source files — safely. Parsing is done with
[tree-sitter](https://tree-sitter.github.io), so comment-like text inside strings, regexes,
or docstrings is never touched. 17 languages supported.

```sh
rm-comments src/main.rs              # strip in place (atomic write)
rm-comments --stdout src/main.rs     # print result, don't modify
rm-comments --check src/main.rs      # exit 1 if changes would be made (CI/hooks)
cat foo.py | rm-comments --stdin --lang py
rm-comments --keep-doc-comments lib.rs   # preserve ///, //!, /** */ docs
```

## Install

```sh
cargo install rm-comments
```

Or build from source: `cargo build --release` → `target/release/rm-comments`.

## Safety guarantees

- Files that don't parse cleanly, or have an unknown extension, are **never modified**.
- Writes are atomic (temp file + rename) — a crash can't leave a truncated file.
- Line endings (LF/CRLF), trailing-newline presence, and a `#!` shebang on line 1 are
  preserved. Idempotent: running twice = running once.
- Everything that isn't a comment survives byte-for-byte, apart from deliberate whitespace
  cleanup: full-line comments are removed including their newline; trailing comments are
  removed along with the gap before them; blank-line runs around removals collapse to at
  most one blank line. The whole policy lives in one function (`rebuild()` in `src/lib.rs`).

### Python docstrings are not comments

Python docstrings are string expression statements in the grammar — they are **correctly
left in place**. Only `#` comments are removed in Python files. By design, not a bug.

### Doc comments

Doc comments that grammars represent as comment nodes (Rust `///`/`//!`, JSDoc/Javadoc
`/** */`, Doxygen `/*!`) are removed by default — "all comments" means all. Pass
`--keep-doc-comments` to preserve them.

## Supported languages

Rust, JavaScript/JSX, TypeScript, TSX, Python, Go, Java, C, C++, C#, Ruby, PHP, HTML, CSS,
Bash/Shell, YAML, TOML. Language is detected from the file extension (case-insensitive).

Not included: JSONC/JSON5 (no published grammar crate currently compatible with modern
tree-sitter; plain JSON has no comments per spec) and SCSS (same problem). Both slot
straight into the recipe below once a compatible crate exists.

### Adding a language

1. `cargo add tree-sitter-<lang>`
2. Add one entry to `LANGUAGES` in `src/languages.rs`:

   ```rust
   Lang {
       name: "kotlin",
       extensions: &["kt", "kts"],
       language: || tree_sitter_kotlin::LANGUAGE.into(),
       comment_kinds: &["comment"], // check the grammar's node-types.json
   },
   ```

3. Add `tests/fixtures/<name>/input.<ext>` and `expected.<ext>` — the golden test picks
   them up automatically (and fails if the fixture pair is missing).

Comment node-kind names differ per grammar — verify against the grammar's
`node-types.json` (most use `"comment"`; Rust and Java use `"line_comment"` +
`"block_comment"`).

## Zed editor integration

`rm-comments` ships as a command in [Zed](https://zed.dev) via a task:

```sh
rm-comments install-zed-task
```

This adds a **Strip Comments** task (pointing at the binary's own absolute path) to
`~/.config/zed/tasks.json` — creating the file, or splicing into an existing one while
preserving your comments and trailing commas (a backup is written first; if the file looks
too unusual to edit safely, it prints the snippet for you to paste instead). Idempotent.

Then in Zed: `cmd-shift-p` → `task: spawn` → **Strip Comments**. The task saves the focused
buffer first (`"save": "current"`), strips the file on disk, and Zed reloads it. For a
one-press keybinding, add [`zed/keymap.json`](zed/keymap.json) to `~/.config/zed/keymap.json`
(`cmd-alt-/` by default). Manual task setup: [`zed/tasks.json`](zed/tasks.json).

### Why a task and not a Zed extension?

Zed's extension API currently has two hard limitations that make the "obvious" WASM
extension impossible:

1. **Extensions cannot read or modify buffer text** — there is no buffer-text API.
2. **Extensions cannot register command-palette actions or tasks.**

So the CLI edits the file on disk and Zed picks up the change — the closest achievable UX
today. The `task: spawn` hop in the palette is a Zed limit, not a design choice. Prefer
spawning over `task::Rerun` (rerun reuses a stale `$ZED_FILE` unless bound with
`"reevaluate_context": true`). If the task fails, the file is untouched; open Zed's
terminal panel to see the error.

## Tests

```sh
cargo test
```

Golden-file tests per language plus edge cases: comment-like text inside strings/regexes,
nested block comments, doc comments (both modes), shebang, CRLF, missing trailing newline,
empty file, no-op files, parse-error refusal, idempotency, and integration tests covering
every CLI flag, exit code, and refusal path.
