# rm-comments

> **AI agents:** see [llms.md](llms.md) for complete installation and usage
> instructions intended for LLMs.

## Overview

`rm-comments` is a command-line tool that removes comments from source code. Files
are parsed with [tree-sitter](https://tree-sitter.github.io) rather than matched with
regular expressions, so removal is grounded in the language's actual syntax:
comment-like sequences inside string literals, regular expressions, and docstrings
are never affected, and everything that is not a comment is preserved byte for byte.

## Example

```sh
rm-comments src/main.rs
```

**Before:**

```rust
/// Loads the configuration from the given path.
fn load(path: &Path) -> Config {
    // First, read the contents of the file into a string.
    let s = read(path); // read the file
    /* Now that we have the string, we can parse it.
       The parse function takes the string and a key. */
    let cfg = parse(&s, "key // not a comment");
    // Finally, return the parsed configuration.
    cfg
}
```

**After:**

```rust
fn load(path: &Path) -> Config {
    let s = read(path);
    let cfg = parse(&s, "key // not a comment");
    cfg
}
```

## Capabilities

- **17 languages** — Rust, JavaScript/TypeScript/JSX, Python, Go, Java, C/C++, C#,
  Ruby, PHP, HTML, CSS, Bash, YAML, and TOML, detected by file extension.
- **Safety** — files that fail to parse are never modified; writes are atomic; line
  endings, shebang lines, and all non-comment content are preserved exactly. Running
  the tool twice produces the same result as running it once.
- **Flexibility** — remove everything, or retain by category: directive comments
  (`eslint-disable`, `# noqa`, `//go:generate`, and similar) are preserved by default
  since removing them changes program behavior; doc comments, user-defined patterns,
  and specific line ranges can each be controlled by flag.
- **AI integration** — ships as a Claude Code plugin whose skill
  ([`SKILL.md`](skills/rm-comments/SKILL.md)) applies a defined policy: comments that
  explain rationale or constraints are kept, comments that narrate what the code
  already expresses are removed.
- **Automation support** — JSON enumeration of every comment, removal by id,
  line-range scoping, and a dry-run mode with conventional exit codes for CI and
  pre-commit use.

## Usage

```sh
rm-comments --check src/main.rs          # report whether changes would be made
rm-comments --keep 'TODO|FIXME' lib.rs   # remove comments except task markers
rm-comments --keep-doc-comments lib.rs   # remove comments except documentation
rm-comments --list lib.rs                # enumerate comments as JSON
rm-comments --apply 2,5,7 lib.rs         # remove the listed comment ids only
```

See `rm-comments help` for the complete flag reference.

## Install

### As an agent skill

#### Claude Code

```
/plugin marketplace add bryceremick/rm-comments
/plugin install rm-comments@rm-comments
```


### As a standalone CLI tool


#### Homebrew (macOS / Linux)

```sh
brew trust bryceremick/tap
brew install bryceremick/tap/rm-comments
```

#### crates.io

```sh
cargo install rm-comments
```

#### cargo-binstall (prebuilt, no compile)

```sh
cargo binstall rm-comments
```

#### Prebuilt binaries

All platforms (incl. Windows), on the
[releases page](https://github.com/bryceremick/rm-comments/releases).

#### From source

```sh
git clone https://github.com/bryceremick/rm-comments
cd rm-comments && cargo build --release
```

The agent skill uses the standalone CLI under the hood — if it's not installed when
the skill first runs, the agent offers to install it for you.


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

This adds a **rm-comments** task (pointing at the binary's own absolute path) to
`~/.config/zed/tasks.json` — creating the file, or splicing into an existing one while
preserving your comments and trailing commas (a backup is written first; if the file looks
too unusual to edit safely, it prints the snippet for you to paste instead). Idempotent.

Then in Zed: `cmd-shift-p` → `task: spawn` → **rm-comments**. The task saves the focused
buffer first (`"save": "current"`), strips the file on disk, and Zed reloads it. For a
one-press keybinding, add [`zed/keymap.json`](zed/keymap.json) to `~/.config/zed/keymap.json`
(`cmd-alt-/` by default). Manual task setup: [`zed/tasks.json`](zed/tasks.json).


## Tests

```sh
cargo test
```

Golden-file tests per language plus edge cases: comment-like text inside strings/regexes,
nested block comments, doc comments (both modes), shebang, CRLF, missing trailing newline,
empty file, no-op files, parse-error refusal, idempotency, and integration tests covering
every CLI flag, exit code, and refusal path.
