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
rm-comments --keep 'TODO|SAFETY' lib.rs  # preserve comments matching a regex
rm-comments --lines 40-80 lib.rs         # only touch comments in a line range
rm-comments --list lib.rs                # enumerate all comments as JSON
rm-comments --apply 2,5,7 lib.rs         # remove exactly those comment ids
```

**Directive comments are preserved by default** — `// eslint-disable`, `# noqa`,
`# type: ignore`, `//go:generate`, `# shellcheck`, `# frozen_string_literal` and friends
carry semantics for other tools, so removing them changes program behavior. Pass
`--strip-directives` to remove them too.

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

## For AI agents

LLMs over-comment. `rm-comments` is built to be the safe mechanical layer under an
agent's judgment: the agent decides *which* comments are noise, the tool guarantees the
removal can't corrupt anything.

The contract:

```sh
rm-comments --list file.rs     # JSON: every comment with id, lines, text, is_doc, is_directive
rm-comments --apply 2,5,7 file.rs   # remove exactly those ids, nothing else
```

The agent reads the JSON, applies its policy (e.g. "keep comments explaining *why*,
drop comments narrating *what*"), and applies the surviving verdict. `--lines A-B` scopes
policy-mode stripping to a region — useful when the agent should only clean code it just
wrote. Ids are positions in the current content; re-run `--list` after any edit.

**Claude Code skill** — [`skills/strip-comments/SKILL.md`](skills/strip-comments/SKILL.md)
packages this workflow with an opinionated keep/remove policy (WHY stays, WHAT goes).
Install it:

```sh
mkdir -p ~/.claude/skills/strip-comments
cp skills/strip-comments/SKILL.md ~/.claude/skills/strip-comments/
```

(or into a project's `.claude/skills/` to share it with the team). Then `/strip-comments`
or just ask the agent to clean up comments.

**Hook recipe (blunt instrument)** — strip every comment from any file Claude Code edits,
automatically. Good for greenfield/solo projects where no comments are wanted at all;
too aggressive for shared codebases (prefer the skill there). In `.claude/settings.json`:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "jq -r '.tool_input.file_path // empty' | xargs -I{} rm-comments {} 2>/dev/null || true"
          }
        ]
      }
    ]
  }
}
```

Directives survive even the blunt instrument (default-on protection), and unparseable or
unsupported files are left untouched, so the hook is safe to fire on everything.

## Tests

```sh
cargo test
```

Golden-file tests per language plus edge cases: comment-like text inside strings/regexes,
nested block comments, doc comments (both modes), shebang, CRLF, missing trailing newline,
empty file, no-op files, parse-error refusal, idempotency, and integration tests covering
every CLI flag, exit code, and refusal path.
