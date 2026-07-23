# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`rm-comments` is a Rust CLI that strips comments from source files using tree-sitter
parsing (not regex). It ships three ways: standalone CLI, a Claude Code plugin skill
(`skills/rm-comments/SKILL.md`), and a Zed editor task. See `llms.md` for the
agent-facing usage contract and `README.md` for user docs.

## Commands

```sh
cargo build --release          # binary at target/release/rm-comments
cargo test                     # all tests
cargo test --test golden       # one test file (golden|policy|edge|cli)
cargo test golden_fixtures     # one test by name
```

CI (`.github/workflows/ci.yml`) just runs `cargo test` on PRs to `main` / pushes to `dev`.

## Architecture

Three source files, ~840 lines:

- **`src/lib.rs`** — the engine. `strip_comments()` and `list_comments()` are the
  public API; `main.rs` and tests both go through them.
- **`src/languages.rs`** — the `LANGUAGES` registry: extension → grammar → comment
  node kinds. This is the only file you touch to add a language.
- **`src/main.rs`** — arg parsing, stdin/file/stdout plumbing, `--list` JSON emission
  (hand-rolled, no serde), atomic writes, and `install-zed-task`. Directory support lives
  here too: `process_one()` handles a single file and `run_dir()` walks a directory (via
  the `ignore` crate, honoring `.gitignore` + skipping hidden dirs), calling `process_one`
  per supported file. The library is untouched by directory mode.

### How stripping works (the important flow)

1. Parse with tree-sitter. **If the tree has any error node, bail without writing** —
   unparseable files are never modified (`collect_comment_ranges`).
2. Walk the tree, collect byte ranges of nodes whose kind is in the language's
   `comment_kinds`. The line-1 `#!` shebang is explicitly excluded even if the grammar
   tokenizes it as a comment.
3. Filter ranges by policy. The default keeps doc comments, directives, task markers, and
   `--keep`-matched comments — each has a `keep_*` flag defaulting on (`--strip-doc-comments`
   / `--strip-directives` / `--strip-markers` opt out); `--lines` scopes by range. `--apply`
   replaces the whole policy with an exact id set.
4. Mask the removed bytes and call **`rebuild()`** — the single source of truth for
   whitespace cleanup (drop full-comment lines incl. newline, trim gaps before trailing
   comments, collapse blank-line runs around removals to at most one). If you're changing
   how output whitespace looks, it's here and nowhere else.

CRLF is normalized to LF for processing and restored at the end; trailing-newline
presence is preserved. Empty result-set → returns the original string byte-for-byte.

### Key invariants (don't break these — tests enforce them)

- **Never modify a file that doesn't parse cleanly or has an unknown extension.**
- **Idempotent**: stripping twice == stripping once (asserted in `golden.rs`).
- **Safety-first defaults: doc comments, directives (`DIRECTIVE_PREFIXES`), and task
  markers (`MARKER_PREFIXES`: TODO/FIXME/HACK/XXX/BUG) are all kept by default** — a bare
  run removes only plain narration. Each opts out via `--strip-doc-comments` /
  `--strip-directives` / `--strip-markers`. Markers match as a whole leading token
  (word boundary), so `// todos` and `// buggy` are not kept.
- **`--list` ids are positional** in current file content; they're invalid after any
  edit. `--apply` ignores all keep policies.
- Exit codes: `0` ok / `1` only for `--check` when changes would be made / `2` error.
  On any error the file is left untouched.

## Adding a language

1. `cargo add tree-sitter-<lang>`
2. Add one `Lang` entry to `LANGUAGES` in `src/languages.rs`. Get `comment_kinds` from
   the grammar's `node-types.json` — most use `"comment"`; Rust/Java use
   `line_comment`+`block_comment`, CSS adds `js_comment`, JS/TS add `html_comment`.
   If you add a new multi-kind shape, also extend `kind_of()` in `lib.rs`.
3. Add `tests/fixtures/<name>/input.<ext>` + `expected.<ext>`. The golden test iterates
   the registry and **fails if the fixture pair is missing**, so this step is mandatory.

## Tests

- `golden.rs` — one fixture pair per registered language; strips input → expects
  `expected`, then re-strips to prove idempotency.
- `policy.rs` — directives, `--keep`, `--lines`, and the list/apply contract.
- `edge.rs` — parser-grounding cases (comment-like text in strings/regexes/f-strings).
- `cli.rs` — spawns the built binary: flags, exit codes, atomic-write / never-corrupt
  guarantees.
