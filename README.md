# rm-comments

> **AI agents:** see [llms.md](llms.md) for complete installation and usage
> instructions intended for LLMs.

## Contents

- [Overview](#overview)
- [Installation Methods](#installation-methods)
- [LLM Agent Plugins](#llm-agent-plugins)
- [Basic Usage](#basic-usage)
- [Command reference](#command-reference)
  - [Common uses](#common-uses)
- [Highlights](#highlights)
- [Safety guarantees](#safety-guarantees)
- [Supported languages](#supported-languages)
  - [Adding a language](#adding-a-language)
- [Zed editor integration](#zed-editor-integration)

## Overview

`rm-comments` is a cli tool that quickly and safely removes comments from source code files.

Files are parsed using the [tree-sitter](https://tree-sitter.github.io) crate rather than matched with
regular expressions, so removal is done using the language's actual syntax.

Comment-like sequences inside string literals, regular expressions, and docstrings
are never affected, and everything that is not a comment is preserved byte for byte.

The tool also ships with a rich set of arguments/options that provide granular control over exactly what comments
are removed or kept from source files. 

## Installation Methods

### Homebrew

```sh
brew trust bryceremick/tap
brew install bryceremick/tap/rm-comments
```

### crates.io

```sh
cargo install rm-comments
```

### cargo-binstall (prebuilt, no compile)

```sh
cargo binstall rm-comments
```

### Prebuilt binaries

All platforms (incl. Windows), on the
[releases page](https://github.com/bryceremick/rm-comments/releases).

### From source

```sh
git clone https://github.com/bryceremick/rm-comments
cd rm-comments && cargo build --release
```

## LLM Agent Plugins

These plugins do not include the binary, only a skill instructing your agent on how and when to utilize this tool. 

> Community contributions are welcome if you don't see your preferred agent here

### Claude Code
```bash
/plugin marketplace add bryceremick/rm-comments
/plugin install rm-comments@rm-comments
```



## Basic Usage

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

OR point it at a directory to clean a whole tree at once:

```sh
rm-comments src/
```

## Command reference

```
rm-comments [OPTIONS] <FILE|DIR>              # strip in place (atomic write)
rm-comments [OPTIONS] --stdin --lang <NAME>   # read stdin, write stdout
rm-comments install-zed-task                  # add the Zed editor task
```

| Flag | Effect |
|---|---|
| `--stdout` | Print the result instead of writing the file (file/stdin only) |
| `--check`, `--dry-run` | Report only; exit 1 if changes would be made, 0 if clean |
| `--stdin` | Read source from stdin, write to stdout; requires `--lang` |
| `--lang <NAME>` | Language name or extension for `--stdin` (e.g. `rust`, `py`) |
| `--keep-doc-comments` | Preserve `///`, `//!`, `/*!`, `/** */` doc comments |
| `--keep <REGEX>` | Preserve comments matching REGEX (repeatable) |
| `--strip-directives` | Also remove directive comments (kept by default) |
| `--lines <A-B>` | Restrict removal to a 1-based line range (repeatable; `N` = single line) |
| `--list` | Print every comment as JSON; modifies nothing |
| `--apply <IDS>` | Remove exactly these comma-separated `--list` ids; ignores keep policies (file only) |

Exit codes: `0` success / nothing to do; `1` (`--check` only) changes would be made;
`2` error (unsupported extension, parse failure, bad flags). On any error the file is
left untouched. For a directory, `2` means at least one file errored — the rest were
still processed.

### Common uses

```sh
# Strip every comment from one file, in place
rm-comments src/main.rs

# Clean a whole project (respects .gitignore, skips hidden dirs)
rm-comments src/

# Preview without writing — CI/pre-commit friendly (exit 1 if anything would change)
rm-comments --check src/

# Strip but keep doc comments and any TODO/FIXME markers
rm-comments --keep-doc-comments --keep 'TODO|FIXME' src/lib.rs

# Only touch the region you just edited
rm-comments --lines 40-80 src/handler.rs

# Pipe through a formatter without touching disk
rm-comments --stdin --lang rust < in.rs > out.rs

# Surgical removal: enumerate, pick ids, remove exactly those
rm-comments --list src/main.rs                 # -> JSON with an id per comment
rm-comments --apply 2,5,7 src/main.rs          # remove ids 2, 5, 7 only
```

## Highlights

- **Safety** — files that fail to parse are never modified; writes are atomic; line
  endings, shebang lines, and all non-comment content are preserved exactly. Running
  the tool twice produces the same result as running it once.
- **Flexibility** — remove everything, or retain by category: directive comments
  (`eslint-disable`, `# noqa`, `//go:generate`, and similar) are preserved by default
  since removing them changes program behavior; doc comments, user-defined patterns,
  and specific line ranges can each be controlled by flag.
- **LLM integration** — this repo also ships a plugin whose skill
  ([`SKILL.md`](skills/rm-comments/SKILL.md)) applies a defined policy: comments that
  explain rationale or constraints are kept, comments that narrate what the code
  already expresses are removed.
- **Automation support** — JSON enumeration of every comment, removal by id,
  line-range scoping, and a dry-run mode with conventional exit codes for CI and
  pre-commit use.

## Safety guarantees

- Files that don't parse cleanly, or have an unknown extension, are **never modified**.
- Writes are atomic (temp file + rename) — a crash can't leave a truncated file.
- Line endings (LF/CRLF), trailing-newline presence, and a `#!` shebang on line 1 are
  preserved. Idempotent: running twice = running once.
- Everything that isn't a comment survives byte-for-byte, apart from deliberate whitespace
  cleanup: full-line comments are removed including their newline; trailing comments are
  removed along with the gap before them; blank-line runs around removals collapse to at
  most one blank line. The whole policy lives in one function (`rebuild()` in `src/lib.rs`).
- Python docstrings are string expression statements in the grammar — they are **correctly
  left in place**. Only `#` comments are removed in Python files. By design, not a bug.
- Doc comments that grammars represent as comment nodes (Rust `///`/`//!`, JSDoc/Javadoc
  `/** */`, Doxygen `/*!`) are removed by default — "all comments" means all. Pass
  `--keep-doc-comments` to preserve them.

## Supported languages

For the authoritative list, see the `LANGUAGES` registry in
[`src/languages.rs`](src/languages.rs) — every supported language, its file extensions,
and grammar live there. Language is detected from the file extension (case-insensitive).

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
